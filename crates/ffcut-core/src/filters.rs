//! Any ffmpeg filter as an effect: which filters the user's ffmpeg has and what options they take
//! (`ffmpeg -filters`, `ffmpeg -h filter=NAME`), and the filter text of a [`FilterFx`].
//!
//! Only plain one-in, one-out filters are offered (audio → audio, video → video), so one fits anywhere in
//! a stack. The option list drives a form; the values stay ffmpeg text, so nothing is lost in a translation.

use crate::error::Error;
pub use crate::model::filter::FilterFx;
use crate::run::{command, run_capture};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Media {
    Audio,
    Video,
}

/// One entry of `ffmpeg -filters`.
#[derive(Debug, Clone, PartialEq)]
pub struct FilterInfo {
    pub name: String,
    pub desc: String,
    pub media: Media,
}

/// Filters that make no sense as an effect: no-ops, debugging aids, format plumbing, and the plugin hosts
/// (those have their own entries, see `plugins`).
const SKIP: &[&str] = &[
    "abench",
    "bench",
    "acopy",
    "copy",
    "anull",
    "null",
    "afifo",
    "fifo",
    "asendcmd",
    "sendcmd",
    "azmq",
    "zmq",
    "ametadata",
    "metadata",
    "aperms",
    "perms",
    "arealtime",
    "realtime",
    "ashowinfo",
    "showinfo",
    "aformat",
    "format",
    "noformat",
    "hwupload",
    "hwdownload",
    "hwmap",
    "ladspa",
    "lv2",
    "frei0r",
    "libplacebo",
];

/// Hardware filters want frames in GPU memory, which these chains never have.
const HW_SUFFIX: &[&str] =
    &["_cuda", "_npp", "_vaapi", "_qsv", "_opencl", "_vulkan", "_videotoolbox", "_vt", "_amf", "_d3d11", "_d3d12", "_rkrga", "_mediacodec"];

fn offered(name: &str) -> bool {
    !SKIP.contains(&name) && !HW_SUFFIX.iter().any(|s| name.ends_with(s))
}

/// What FFcut's own controls already do: crop, resize, rotate, trim, fades, volume, frame rate, placement.
const MAIN_UI: &[&str] = &["crop", "scale", "rotate", "transpose", "hflip", "vflip", "pad", "fps", "fade", "afade", "volume", "adelay", "trim", "atrim"];

/// The filters behind the ready-made effects that are not in `builtin` (equalizer, compressor, colour...).
const OWN_EFFECTS: &[&str] = &[
    "equalizer",
    "lowpass",
    "highpass",
    "lowshelf",
    "highshelf",
    "acompressor",
    "deesser",
    "afftdn",
    "loudnorm",
    "eq",
    "curves",
    "vignette",
    "unsharp",
    "cas",
    "hqdn3d",
    "deblock",
    "deband",
    "deflicker",
    "drawtext",
    "drawbox",
];

/// Filters that change the length, speed or frame count: the trim, the timeline and the preview count on
/// every second and frame staying where it is.
const RETIME: &[&str] = &[
    "atempo",
    "setpts",
    "asetpts",
    "asetrate",
    "reverse",
    "areverse",
    "loop",
    "aloop",
    "apad",
    "tpad",
    "framerate",
    "framestep",
    "minterpolate",
    "decimate",
    "mpdecimate",
    "select",
    "aselect",
    "silenceremove",
    "thumbnail",
    "shuffleframes",
    "telecine",
    "detelecine",
    "pullup",
    "fieldmatch",
    "dejudder",
    "separatefields",
    "weave",
    "doubleweave",
    "tinterlace",
    "interlace",
    "repeatfields",
    "palettegen",
    "asetnsamples",
    "aresample",
    "settb",
    "asettb",
];

/// Filters that only measure or draw a measurement (scopes, statistics, detectors), or are debugging aids:
/// nothing to hear or see in an export.
const ANALYSIS: &[&str] = &[
    "astats",
    "aspectralstats",
    "drmeter",
    "replaygain",
    "ebur128",
    "asr",
    "signalstats",
    "histogram",
    "thistogram",
    "waveform",
    "vectorscope",
    "oscilloscope",
    "datascope",
    "pixscope",
    "ciescope",
    "showpalette",
    "drawgraph",
    "adrawgraph",
    "graphmonitor",
    "agraphmonitor",
    "bbox",
    "blackframe",
    "scdet",
    "idet",
    "entropy",
    "vmafmotion",
    "siti",
    "mestimate",
    "find_rect",
    "readeia608",
    "readvitc",
    "vfrdet",
    "bitplanenoise",
    "vidstabtransform",
    "dnn_classify",
    "pixdesctest",
    "codecview",
    "latency",
    "alatency",
    "cue",
    "acue",
    "sidedata",
    "asidedata",
];

/// Is filter `name` worth listing as a plain "FFmpeg filter"? Not when FFcut has controls for what it does
/// (its main tools or a ready-made effect), when it changes the timing, or when it only measures.
/// An effect made with a hidden filter still works: it is only not offered.
pub fn in_picker(name: &str) -> bool {
    !MAIN_UI.contains(&name)
        && !OWN_EFFECTS.contains(&name)
        && !crate::builtin::covers(name)
        && !RETIME.contains(&name)
        && !ANALYSIS.contains(&name)
        && !name.ends_with("detect")
}

/// The one-in, one-out audio and video filters of `ffmpeg -filters`, in its order (alphabetical).
pub fn parse_filter_list(text: &str) -> Vec<FilterInfo> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let (Some(_flags), Some(name), Some(io)) = (it.next(), it.next(), it.next()) else { continue };
        let media = match io {
            "A->A" => Media::Audio,
            "V->V" => Media::Video,
            _ => continue,
        };
        if offered(name) && valid_name(name) {
            out.push(FilterInfo { name: name.to_string(), desc: it.collect::<Vec<_>>().join(" "), media });
        }
    }
    out
}

/// Which of the plugin hosts (`ladspa`, `lv2`, `frei0r`) are in `ffmpeg -filters`.
pub fn parse_plugin_hosts(text: &str) -> Vec<String> {
    text.lines().filter_map(|l| l.split_whitespace().nth(1)).filter(|n| ["ladspa", "lv2", "frei0r"].contains(n)).map(str::to_string).collect()
}

/// Runs `ffmpeg -filters`: the offered filters and the plugin hosts.
pub fn list_filters_and_hosts(ffmpeg: &Path) -> Result<(Vec<FilterInfo>, Vec<String>), Error> {
    let out = run_capture("ffmpeg", command(ffmpeg).args(["-hide_banner", "-filters"]))?;
    let text = String::from_utf8_lossy(&out.stdout);
    Ok((parse_filter_list(&text), parse_plugin_hosts(&text)))
}

/// Runs `ffmpeg -filters`.
pub fn list_filters(ffmpeg: &Path) -> Result<Vec<FilterInfo>, Error> {
    list_filters_and_hosts(ffmpeg).map(|(f, _)| f)
}

// ------------------------------------------------------------------ options of one filter

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptKind {
    Int,
    Float,
    Bool,
    /// Names joined by `+` (the choices are the names).
    Flags,
    Color,
    /// Anything ffmpeg reads from text: strings, expressions, sizes, rates, durations, layouts...
    Text,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub name: String,
    pub help: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptInfo {
    pub name: String,
    pub kind: OptKind,
    /// ffmpeg's own type name (`int`, `duration`, `image_size`...).
    pub type_name: String,
    pub help: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// As ffmpeg prints it (a number, a choice name, `false`, a string without its quotes).
    pub default: Option<String>,
    /// Named values (for `Int` options) or the flag names (for `Flags`).
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FilterHelp {
    pub name: String,
    pub desc: String,
    pub options: Vec<OptInfo>,
}

/// A limit as ffmpeg prints it (`-1`, `1.5`, `INT_MAX`, `-FLT_MAX`...).
fn bound(s: &str) -> Option<f64> {
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let v = match s {
        "INT_MAX" => i32::MAX as f64,
        "INT_MIN" => i32::MIN as f64,
        "UINT32_MAX" => u32::MAX as f64,
        "I64_MAX" => i64::MAX as f64,
        "I64_MIN" => i64::MIN as f64,
        "FLT_MAX" => f32::MAX as f64,
        "FLT_MIN" => f32::MIN_POSITIVE as f64,
        "DBL_MAX" => f64::MAX,
        "DBL_MIN" => f64::MIN_POSITIVE,
        _ => s.parse::<f64>().ok()?,
    };
    Some(if neg { -v } else { v })
}

/// The ffmpeg flag column (`..F.A....T.`): dots and capital letters (a choice may be called `REPLAYGAIN`).
fn is_flag_column(s: &str) -> bool {
    s.len() >= 8 && s.contains('.') && s.chars().all(|c| c == '.' || c.is_ascii_uppercase())
}

/// Splits `set the gain (from 0 to 1) (default 0.5)` into the text and its range / default.
fn split_help(help: &str) -> (String, Option<f64>, Option<f64>, Option<String>) {
    let mut text = help.trim().to_string();
    let mut default = None;
    if let Some(i) = text.rfind("(default ") {
        if text.ends_with(')') {
            let d = text[i + 9..text.len() - 1].trim();
            let d = d.strip_prefix('"').and_then(|d| d.strip_suffix('"')).unwrap_or(d);
            default = Some(d.to_string());
            text.truncate(i);
        }
    }
    let (mut min, mut max) = (None, None);
    if let Some(i) = text.rfind("(from ") {
        if let Some((a, b)) = text[i + 6..].trim_end().strip_suffix(')').and_then(|r| r.split_once(" to ")) {
            min = bound(a.trim());
            max = bound(b.trim());
            text.truncate(i);
        }
    }
    (text.trim().to_string(), min, max, default)
}

fn kind_of(type_name: &str) -> OptKind {
    match type_name {
        "int" | "int64" | "uint" | "uint64" => OptKind::Int,
        "float" | "double" => OptKind::Float,
        "boolean" => OptKind::Bool,
        "flags" => OptKind::Flags,
        "color" => OptKind::Color,
        _ => OptKind::Text,
    }
}

/// Reads `ffmpeg -h filter=NAME`.
pub fn parse_filter_help(text: &str) -> FilterHelp {
    let mut h = FilterHelp::default();
    let mut in_opts = false;
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if let Some(n) = line.strip_prefix("Filter ") {
            h.name = n.trim().to_string();
            if let Some(d) = lines.peek().filter(|l| l.starts_with("  ") && !l.trim().is_empty()) {
                h.desc = d.trim().to_string();
                lines.next();
            }
            continue;
        }
        if line.trim_end().ends_with("AVOptions:") {
            in_opts = true;
            continue;
        }
        if !in_opts {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let toks: Vec<&str> = line.split_whitespace().collect();
        if toks.is_empty() || indent < 3 {
            in_opts = false;
            continue;
        }
        if indent <= 3 {
            // name <type> flags help...
            let Some(t) = toks.get(1).and_then(|t| t.strip_prefix('<')).and_then(|t| t.strip_suffix('>')) else {
                continue;
            };
            let rest = toks.get(2).filter(|f| is_flag_column(f)).map_or(&toks[2.min(toks.len())..], |_| &toks[3.min(toks.len())..]);
            let (help, min, max, default) = split_help(&rest.join(" "));
            // an alias (`f` for `file`) repeats the line before it
            if h.options.last().is_some_and(|p| p.help == help && p.type_name == t && p.default == default) {
                continue;
            }
            h.options.push(OptInfo { name: toks[0].to_string(), kind: kind_of(t), type_name: t.to_string(), help, min, max, default, choices: Vec::new() });
        } else if let Some(o) = h.options.last_mut() {
            // name [value] flags help...
            let at = toks.iter().position(|t| is_flag_column(t)).unwrap_or(toks.len().min(2));
            let help = toks.get(at + 1..).map(|r| r.join(" ")).unwrap_or_default();
            if !o.choices.iter().any(|c| c.name == toks[0]) {
                o.choices.push(Choice { name: toks[0].to_string(), help });
            }
        }
    }
    h
}

/// Runs `ffmpeg -h filter=NAME`.
pub fn filter_help(ffmpeg: &Path, name: &str) -> Result<FilterHelp, Error> {
    if !valid_name(name) {
        return Err(Error::Invalid(format!("“{name}” is not a filter name")));
    }
    let out = run_capture("ffmpeg", command(ffmpeg).args(["-hide_banner", "-h", &format!("filter={name}")]))?;
    let h = parse_filter_help(&String::from_utf8_lossy(&out.stdout));
    if h.name.is_empty() {
        return Err(Error::Invalid(format!("this ffmpeg has no filter “{name}”")));
    }
    Ok(h)
}

// ------------------------------------------------------------------ filter text

/// Filter names: letters, digits and `_`.
pub fn valid_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Option names: also `-` (`compand` has a `soft-knee`).
pub fn valid_key(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Escapes a value for a filter option inside `'...'` (found by experiment against real ffmpeg, which
/// parses the text twice): `\` doubles, `:` gets a backslash, and a `'` leaves the quote and comes back.
pub(crate) fn quote_value(s: &str) -> String {
    let t = s.replace('\\', "\\\\").replace(':', "\\:").replace('\'', "'\\\\\\''");
    format!("'{t}'")
}

/// A value as it goes after `key=`: as it is when nothing in it means anything to ffmpeg, else quoted.
pub fn opt_value(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "._-+#@/*^|".contains(c)) {
        s.to_string()
    } else {
        quote_value(s)
    }
}

/// `name` or `name=key=value:key=value`; `None` when the name cannot be a filter.
pub fn filter_text(f: &FilterFx) -> Option<String> {
    if !valid_name(&f.name) {
        return None;
    }
    let opts: Vec<String> = f.opts.iter().filter(|(k, _)| valid_key(k)).map(|(k, v)| format!("{k}={}", opt_value(v))).collect();
    Some(if opts.is_empty() { f.name.clone() } else { format!("{}={}", f.name, opts.join(":")) })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = " ... abench            A->A       Benchmark part of a filtergraph.
 ..C acompressor       A->A       Audio compressor.
 ... acrossfade        AA->A      Cross fade two input audio streams.
 .S. acrossover        A->N       Split audio into per-bands streams.
 TSC adynamicequalizer A->A       Apply Dynamic Equalization of input audio.
 TSC colorbalance      V->V       Adjust the color balance.
 ... scale_cuda        V->V       GPU accelerated video resizer
 ... ladspa            N->A       Apply LADSPA effect.
 ... frei0r            V->V       Apply a frei0r effect.
 |.. testsrc           |->V       Generate test pattern.";

    #[test]
    fn the_list_keeps_one_in_one_out_effects() {
        let l = parse_filter_list(LIST);
        let names: Vec<&str> = l.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["acompressor", "adynamicequalizer", "colorbalance"]);
        assert_eq!(l[0].media, Media::Audio);
        assert_eq!(l[2], FilterInfo { name: "colorbalance".into(), desc: "Adjust the color balance.".into(), media: Media::Video });
        assert_eq!(parse_plugin_hosts(LIST), ["ladspa", "frei0r"]);
    }

    const HELP: &str = "Filter acompressor
  Audio compressor.
    Inputs:
       #0: default (audio)
    Outputs:
       #0: default (audio)
acompressor/sidechaincompress AVOptions:
   level_in          <double>     ..F.A....T. set input gain (from 0.015625 to 64) (default 1)
   mode              <int>        ..F.A....T. set mode (from 0 to 1) (default downward)
     downward        0            ..F.A....T.
     upward          1            ..F.A....T. louder
     REPLAYGAIN      5            ..F.A......
   file              <string>     ..F.A...... set library name or full path
   f                 <string>     ..F.A...... set library name or full path
   size              <int>        ..F.A...... set size (from 1 to INT_MAX) (default 1024)
   pl                <boolean>    ..FV.....T. preserve lightness (default false)
   delays            <string>     ..F.A...... set list of signal delays (default \"1000\")
   gain              <float>      ..FV....... set gain (from -FLT_MAX to FLT_MAX) (default 0)
   flags             <flags>      ..FV....... set flags (default a+b)
     a                            ..FV....... first
     b                            ..FV....... second

This filter has support for timeline through the 'enable' option.
";

    #[test]
    fn options_with_ranges_defaults_choices_and_no_aliases() {
        let h = parse_filter_help(HELP);
        assert_eq!((h.name.as_str(), h.desc.as_str()), ("acompressor", "Audio compressor."));
        let names: Vec<&str> = h.options.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["level_in", "mode", "file", "size", "pl", "delays", "gain", "flags"]);
        let o = &h.options[0];
        assert_eq!((o.kind, o.help.as_str(), o.min, o.max, o.default.as_deref()), (OptKind::Float, "set input gain", Some(0.015625), Some(64.0), Some("1")));
        let m = &h.options[1];
        assert_eq!(m.default.as_deref(), Some("downward"));
        let c = |n: &str, h: &str| Choice { name: n.into(), help: h.into() };
        assert_eq!(m.choices, [c("downward", ""), c("upward", "louder"), c("REPLAYGAIN", "")]);
        assert_eq!((h.options[2].kind, h.options[2].default.as_ref()), (OptKind::Text, None));
        assert_eq!(h.options[3].max, Some(i32::MAX as f64));
        assert_eq!((h.options[4].kind, h.options[4].default.as_deref()), (OptKind::Bool, Some("false")));
        assert_eq!(h.options[5].default.as_deref(), Some("1000"));
        assert_eq!(h.options[6].min, Some(-(f32::MAX as f64)));
        let f = &h.options[7];
        assert_eq!((f.kind, f.choices.len(), f.choices[1].help.as_str()), (OptKind::Flags, 2, "second"));
    }

    #[test]
    fn the_picker_leaves_out_what_ffcut_does_itself() {
        for hidden in ["crop", "atempo", "equalizer", "aecho", "lut3d", "silencedetect", "cropdetect", "astats", "waveform", "setpts"] {
            assert!(!in_picker(hidden), "{hidden}");
        }
        for shown in ["colorbalance", "acontrast", "pan", "stereotools", "colorchannelmixer", "bandreject", "nlmeans"] {
            assert!(in_picker(shown), "{shown}");
        }
    }

    #[test]
    fn filter_text_quotes_only_what_needs_it() {
        let mut f = FilterFx::new("aecho");
        assert_eq!(filter_text(&f).unwrap(), "aecho");
        f.set("in_gain", Some("0.8".into()));
        f.set("delays", Some("1000|1800".into()));
        f.set("expr", Some("between(t,1,2):x".into()));
        assert_eq!(filter_text(&f).unwrap(), r"aecho=in_gain=0.8:delays=1000|1800:expr='between(t,1,2)\:x'");
        f.set("expr", None);
        f.set("bad key", Some("1".into()));
        assert_eq!(filter_text(&f).unwrap(), "aecho=in_gain=0.8:delays=1000|1800");
        assert!(filter_text(&FilterFx::new("a,b")).is_none());
        assert!(filter_text(&FilterFx::new("")).is_none());
    }
}
