//! Opens a LADSPA or a frei0r library and reads what it says about itself. Runs in the probe process only
//! (see [`super::probe_main`]): loading a library runs its code.

use super::{Control, CtlKind, PluginInfo, PluginKind};
use crate::fmtnum::num;
use std::ffi::{c_char, c_int, c_ulong, c_void, CStr};
use std::path::Path;

fn text(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: plugins hand out NUL-terminated strings that live as long as the library
    unsafe { CStr::from_ptr(p) }.to_string_lossy().trim().to_string()
}

/// What `path` holds: LADSPA plugins, a frei0r plugin, or nothing (not a plugin library).
pub fn probe_file(path: &Path) -> Result<Vec<PluginInfo>, String> {
    // SAFETY: loading runs the library's initialisers; that is why this runs in a separate process
    let lib = unsafe { libloading::Library::new(path) }.map_err(|e| e.to_string())?;
    let r = unsafe {
        if let Ok(f) = lib.get::<LadspaDescriptorFn>(b"ladspa_descriptor\0") {
            Ok(ladspa(path, *f))
        } else if lib.get::<unsafe extern "C" fn() -> c_int>(b"f0r_init\0").is_ok() {
            frei0r(path, &lib).map(|p| vec![p])
        } else {
            Ok(Vec::new())
        }
    };
    // some plugins do not survive being unloaded; the process ends soon anyway
    std::mem::forget(lib);
    r
}

// ------------------------------------------------------------------ LADSPA (ladspa.h)

#[repr(C)]
struct LadspaHint {
    hint: c_int,
    lower: f32,
    upper: f32,
}

#[repr(C)]
struct LadspaDescriptor {
    unique_id: c_ulong,
    label: *const c_char,
    properties: c_int,
    name: *const c_char,
    maker: *const c_char,
    copyright: *const c_char,
    port_count: c_ulong,
    port_descriptors: *const c_int,
    port_names: *const *const c_char,
    port_range_hints: *const LadspaHint,
    // more fields follow; they are not read
}

type LadspaDescriptorFn = unsafe extern "C" fn(c_ulong) -> *const LadspaDescriptor;

const PORT_INPUT: c_int = 0x1;
const PORT_OUTPUT: c_int = 0x2;
const PORT_CONTROL: c_int = 0x4;
const PORT_AUDIO: c_int = 0x8;
const HINT_BELOW: c_int = 0x1;
const HINT_ABOVE: c_int = 0x2;
const HINT_TOGGLED: c_int = 0x4;
const HINT_SAMPLE_RATE: c_int = 0x8;
const HINT_LOG: c_int = 0x10;
const HINT_INTEGER: c_int = 0x20;
const HINT_DEFAULT: c_int = 0x3C0;

/// The default of a control, worked out as ffmpeg's `set_default_ctl_value` does.
fn ladspa_default(hint: c_int, lo: f32, hi: f32) -> Option<f64> {
    let (lo, hi) = (lo as f64, hi as f64);
    let log = hint & HINT_LOG != 0;
    let mix = |a: f64| if log { (lo.ln() * a + hi.ln() * (1.0 - a)).exp() } else { lo * a + hi * (1.0 - a) };
    Some(match hint & HINT_DEFAULT {
        0x40 => lo,
        0x80 => mix(0.75),
        0xC0 => mix(0.5),
        0x100 => mix(0.25),
        0x140 => hi,
        0x200 => 0.0,
        0x240 => 1.0,
        0x280 => 100.0,
        0x2C0 => 440.0,
        _ => return None,
    })
}

unsafe fn ladspa(path: &Path, f: LadspaDescriptorFn) -> Vec<PluginInfo> {
    let mut out = Vec::new();
    for i in 0..4096 {
        let d = f(i as c_ulong);
        if d.is_null() {
            break;
        }
        let d = &*d;
        let (mut inputs, mut outputs, mut controls) = (0, 0, Vec::new());
        for p in 0..d.port_count as usize {
            let pd = *d.port_descriptors.add(p);
            if pd & PORT_AUDIO != 0 {
                if pd & PORT_INPUT != 0 {
                    inputs += 1;
                } else if pd & PORT_OUTPUT != 0 {
                    outputs += 1;
                }
            } else if pd & PORT_CONTROL != 0 && pd & PORT_INPUT != 0 {
                let h = &*d.port_range_hints.add(p);
                let (below, above) = (h.hint & HINT_BELOW != 0, h.hint & HINT_ABOVE != 0);
                let toggled = h.hint & HINT_TOGGLED != 0;
                let int = h.hint & HINT_INTEGER != 0;
                let def = ladspa_default(h.hint, h.lower, h.upper);
                let kind = if toggled {
                    CtlKind::Toggle
                } else if int {
                    CtlKind::Int
                } else {
                    CtlKind::Float
                };
                let digits = if int || toggled { 0 } else { 6 };
                controls.push(Control {
                    key: format!("c{}", controls.len()),
                    name: text(*d.port_names.add(p)),
                    help: String::new(),
                    kind,
                    min: below.then_some(h.lower as f64),
                    max: above.then_some(h.upper as f64),
                    default: def.map(|v| num(v, digits)),
                    strict: true,
                    log: h.hint & HINT_LOG != 0,
                    sample_rate: h.hint & HINT_SAMPLE_RATE != 0,
                    choices: Vec::new(),
                });
            }
        }
        let problem = if inputs == 0 {
            Some("makes sound instead of changing it: it has no audio input".to_string())
        } else if outputs == 0 {
            Some("has no audio output".to_string())
        } else {
            None
        };
        out.push(PluginInfo {
            kind: PluginKind::Ladspa,
            id: path.to_string_lossy().into_owned(),
            label: text(d.label),
            name: text(d.name),
            desc: text(d.maker),
            file: path.to_path_buf(),
            inputs,
            outputs,
            controls,
            problem,
        });
    }
    out
}

// ------------------------------------------------------------------ frei0r (frei0r.h)

#[repr(C)]
struct F0rPluginInfo {
    name: *const c_char,
    author: *const c_char,
    plugin_type: c_int,
    color_model: c_int,
    frei0r_version: c_int,
    major_version: c_int,
    minor_version: c_int,
    num_params: c_int,
    explanation: *const c_char,
}

#[repr(C)]
struct F0rParamInfo {
    name: *const c_char,
    kind: c_int,
    explanation: *const c_char,
}

const F0R_FILTER: c_int = 0;
const F0R_BOOL: c_int = 0;
const F0R_DOUBLE: c_int = 1;
const F0R_COLOR: c_int = 2;
const F0R_POSITION: c_int = 3;
const F0R_STRING: c_int = 4;

unsafe fn frei0r(path: &Path, lib: &libloading::Library) -> Result<PluginInfo, String> {
    let sym = |n: &[u8]| -> Result<*const c_void, String> {
        lib.get::<*const c_void>(n).map(|s| *s).map_err(|_| format!("not a frei0r plugin: no {}", String::from_utf8_lossy(&n[..n.len() - 1])))
    };
    let init: unsafe extern "C" fn() -> c_int = std::mem::transmute(sym(b"f0r_init\0")?);
    let info: unsafe extern "C" fn(*mut F0rPluginInfo) = std::mem::transmute(sym(b"f0r_get_plugin_info\0")?);
    let pinfo: unsafe extern "C" fn(*mut F0rParamInfo, c_int) = std::mem::transmute(sym(b"f0r_get_param_info\0")?);
    let construct: unsafe extern "C" fn(u32, u32) -> *mut c_void = std::mem::transmute(sym(b"f0r_construct\0")?);
    let destruct: unsafe extern "C" fn(*mut c_void) = std::mem::transmute(sym(b"f0r_destruct\0")?);
    let get: unsafe extern "C" fn(*mut c_void, *mut c_void, c_int) = std::mem::transmute(sym(b"f0r_get_param_value\0")?);
    for n in [&b"f0r_set_param_value\0"[..], b"f0r_deinit\0"] {
        sym(n)?;
    }
    // mixers have only `f0r_update2`, which ffmpeg does not call
    let has_update = sym(b"f0r_update\0").is_ok();
    if init() < 0 {
        return Err("the plugin failed to start (f0r_init)".into());
    }
    let mut pi: F0rPluginInfo = std::mem::zeroed();
    info(&mut pi);
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    // the starting values: read from an instance
    let inst = if pi.plugin_type == F0R_FILTER { construct(320, 240) } else { std::ptr::null_mut() };
    let mut controls = Vec::new();
    for i in 0..pi.num_params.clamp(0, 256) {
        let mut p: F0rParamInfo = std::mem::zeroed();
        pinfo(&mut p, i);
        let (kind, min, max) = match p.kind {
            F0R_BOOL => (CtlKind::Toggle, None, None),
            F0R_DOUBLE => (CtlKind::Float, Some(0.0), Some(1.0)),
            F0R_COLOR => (CtlKind::Color, None, None),
            F0R_POSITION => (CtlKind::Position, None, None),
            // an unknown type is kept as text: the values are positional, none can be skipped
            _ => (CtlKind::Text, None, None),
        };
        let default = (!inst.is_null()).then(|| {
            let mut buf = [0f64; 4];
            let v = buf.as_mut_ptr() as *mut c_void;
            get(inst, v, i);
            match p.kind {
                F0R_BOOL => if buf[0] >= 0.5 { "y" } else { "n" }.to_string(),
                F0R_DOUBLE => num(buf[0], 6),
                F0R_COLOR => {
                    let c = std::slice::from_raw_parts(v as *const f32, 3);
                    format!("{}/{}/{}", num(c[0] as f64, 4), num(c[1] as f64, 4), num(c[2] as f64, 4))
                }
                F0R_POSITION => format!("{}/{}", num(buf[0], 4), num(buf[1], 4)),
                F0R_STRING => text(*(v as *const *const c_char)),
                _ => String::new(),
            }
        });
        controls.push(Control {
            key: i.to_string(),
            name: text(p.name),
            help: text(p.explanation),
            kind,
            min,
            max,
            default,
            strict: false,
            log: false,
            sample_rate: false,
            choices: Vec::new(),
        });
    }
    if !inst.is_null() {
        destruct(inst);
    }
    let (author, expl) = (text(pi.author), text(pi.explanation));
    let problem = match pi.plugin_type {
        F0R_FILTER if has_update => None,
        F0R_FILTER => Some("has no f0r_update, which ffmpeg calls".to_string()),
        1 => Some("makes a picture instead of changing one (a source)".to_string()),
        2 | 3 => Some("mixes several videos".to_string()),
        _ => Some("of an unknown type".to_string()),
    };
    Ok(PluginInfo {
        kind: PluginKind::Frei0r,
        id: stem.clone(),
        label: String::new(),
        name: Some(text(pi.name)).filter(|n| !n.is_empty()).unwrap_or(stem),
        desc: [expl, author].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" — "),
        file: path.to_path_buf(),
        inputs: 1,
        outputs: 1,
        controls,
        problem,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_as_ffmpeg_works_them_out() {
        assert_eq!(ladspa_default(0x40, 1.0, 5.0), Some(1.0));
        assert_eq!(ladspa_default(0xC0, 0.0, 10.0), Some(5.0));
        assert_eq!(ladspa_default(0x80, 0.0, 4.0), Some(1.0));
        let v = ladspa_default(0xC0 | HINT_LOG, 1.0, 100.0).unwrap();
        assert!((v - 10.0).abs() < 1e-9);
        assert_eq!(ladspa_default(0x2C0, 0.0, 1.0), Some(440.0));
        assert_eq!(ladspa_default(0, 0.0, 1.0), None);
    }
}
