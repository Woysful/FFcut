//! Ready-made effects with their own controls: what each one is (its controls, where it goes in the
//! "+ Add effect" menu, which ffmpeg filters it stands for) and the filter text it makes.
//!
//! The table is the whole effect: the UI draws the controls from it and the chain builders call
//! [`filter_text`], so adding one is adding an entry to [`DEFS`].

use crate::filters::{quote_value, Media};
use crate::fmtnum;
pub use crate::model::builtin::BuiltinFx;

/// The kind of control of one setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ctl {
    /// A number from `min` to `max`; `pct` shows 0..1 as 0..100 %.
    Num {
        min: f64,
        max: f64,
        unit: &'static str,
        decimals: usize,
        log: bool,
        pct: bool,
    },
    Int {
        min: i64,
        max: i64,
        unit: &'static str,
    },
    /// One of these (the value is the position).
    Choice(&'static [&'static str]),
    Toggle,
    /// The effect's file ([`BuiltinFx::file`]): the extensions it opens.
    File(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Param {
    pub key: &'static str,
    pub label: &'static str,
    pub tip: &'static str,
    pub default: f64,
    pub ctl: Ctl,
}

impl Param {
    /// `v` brought into the range of the control (a choice / toggle to a valid position).
    pub fn clamp(&self, v: f64) -> f64 {
        let v = if v.is_finite() { v } else { self.default };
        match self.ctl {
            Ctl::Num { min, max, .. } => v.clamp(min, max),
            Ctl::Int { min, max, .. } => v.round().clamp(min as f64, max as f64),
            Ctl::Choice(c) => v.round().clamp(0.0, c.len().saturating_sub(1) as f64),
            Ctl::Toggle => (v >= 0.5) as u8 as f64,
            Ctl::File(_) => 0.0,
        }
    }
}

pub struct Def {
    /// Stored in the settings: never change it.
    pub id: &'static str,
    pub media: Media,
    /// The heading it is listed under in the "+ Add effect" menu.
    pub group: &'static str,
    pub name: &'static str,
    /// One line for the menu.
    pub desc: &'static str,
    /// Also found by the menu's search (the ffmpeg filters it uses, other names for it).
    pub keywords: &'static str,
    pub params: &'static [Param],
    /// ffmpeg filters this effect does: the plain "FFmpeg filter" list leaves them out.
    pub covers: &'static [&'static str],
    build: fn(&Vals) -> Option<String>,
}

impl Def {
    /// A new effect with every control at its default.
    pub fn new_fx(&self) -> BuiltinFx {
        BuiltinFx {
            id: self.id.to_string(),
            vals: self.params.iter().filter(|p| !matches!(p.ctl, Ctl::File(_))).map(|p| (p.key.to_string(), p.default)).collect(),
            file: String::new(),
        }
    }
    pub fn param(&self, key: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.key == key)
    }
    /// The value of control `key` of `fx` (its default when not set), in its range.
    pub fn value(&self, fx: &BuiltinFx, key: &str) -> f64 {
        self.param(key).map_or(0.0, |p| p.clamp(fx.get(key).unwrap_or(p.default)))
    }
}

impl std::fmt::Debug for Def {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Def").field("id", &self.id).finish()
    }
}

pub fn def(id: &str) -> Option<&'static Def> {
    DEFS.iter().find(|d| d.id == id)
}

pub fn defs(media: Media) -> impl Iterator<Item = &'static Def> {
    DEFS.iter().filter(move |d| d.media == media)
}

/// Some ready-made effect does what ffmpeg filter `name` does.
pub fn covers(name: &str) -> bool {
    DEFS.iter().any(|d| d.covers.contains(&name))
}

/// The filters of `fx` for a `media` chain, joined with `,`; `None` when it changes nothing (or is unknown).
pub fn filter_text(fx: &BuiltinFx, media: Media) -> Option<String> {
    let d = def(&fx.id).filter(|d| d.media == media)?;
    (d.build)(&Vals { def: d, fx })
}

/// The settings of one effect as its build function reads them.
struct Vals<'a> {
    def: &'a Def,
    fx: &'a BuiltinFx,
}

impl Vals<'_> {
    fn n(&self, key: &str) -> f64 {
        self.def.value(self.fx, key)
    }
    fn i(&self, key: &str) -> usize {
        self.n(key) as usize
    }
    fn on(&self, key: &str) -> bool {
        self.n(key) >= 0.5
    }
}

// ------------------------------------------------------------------ helpers

fn f(v: f64) -> String {
    fmtnum::num(v, 4)
}

fn f6(v: f64) -> String {
    fmtnum::num(v, 6)
}

fn lin(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

fn join(parts: Vec<String>) -> Option<String> {
    (!parts.is_empty()).then(|| parts.join(","))
}

// one line per control in the table reads better than a builder
#[allow(clippy::too_many_arguments)]
const fn num(key: &'static str, label: &'static str, tip: &'static str, default: f64, min: f64, max: f64, unit: &'static str, decimals: usize) -> Param {
    Param { key, label, tip, default, ctl: Ctl::Num { min, max, unit, decimals, log: false, pct: false } }
}

#[allow(clippy::too_many_arguments)]
const fn log(key: &'static str, label: &'static str, tip: &'static str, default: f64, min: f64, max: f64, unit: &'static str, decimals: usize) -> Param {
    Param { key, label, tip, default, ctl: Ctl::Num { min, max, unit, decimals, log: true, pct: false } }
}

/// 0..1, shown as a percentage.
const fn pct(key: &'static str, label: &'static str, tip: &'static str, default: f64) -> Param {
    Param { key, label, tip, default, ctl: Ctl::Num { min: 0.0, max: 1.0, unit: " %", decimals: 0, log: false, pct: true } }
}

const fn int(key: &'static str, label: &'static str, tip: &'static str, default: i64, min: i64, max: i64, unit: &'static str) -> Param {
    Param { key, label, tip, default: default as f64, ctl: Ctl::Int { min, max, unit } }
}

const fn choice(key: &'static str, label: &'static str, tip: &'static str, default: usize, options: &'static [&'static str]) -> Param {
    Param { key, label, tip, default: default as f64, ctl: Ctl::Choice(options) }
}

const fn toggle(key: &'static str, label: &'static str, tip: &'static str, on: bool) -> Param {
    Param { key, label, tip, default: on as u8 as f64, ctl: Ctl::Toggle }
}

// ------------------------------------------------------------------ audio

fn limiter(v: &Vals) -> Option<String> {
    Some(format!(
        "alimiter=level_in={}:limit={}:attack=5:release={}:level=0",
        f6(lin(v.n("gain"))),
        f6(lin(v.n("ceiling")).clamp(0.0625, 1.0)),
        f(v.n("release"))
    ))
}

fn gate(v: &Vals) -> Option<String> {
    Some(format!(
        "agate=threshold={}:range={}:ratio=10:attack={}:release={}",
        f6(lin(v.n("threshold"))),
        f6(lin(-v.n("reduction"))),
        f(v.n("attack")),
        f(v.n("release"))
    ))
}

fn bass_treble(v: &Vals) -> Option<String> {
    let mut p = Vec::new();
    if v.n("bass").abs() >= 0.05 {
        p.push(format!("bass=g={}:f={}", f(v.n("bass")), f(v.n("bass_f"))));
    }
    if v.n("treble").abs() >= 0.05 {
        p.push(format!("treble=g={}:f={}", f(v.n("treble")), f(v.n("treble_f"))));
    }
    join(p)
}

/// `aecho` with these taps (delay ms, level); the dry sound is lowered a little as the echoes add up.
fn echo_taps(taps: impl IntoIterator<Item = (f64, f64)>) -> Option<String> {
    let taps: Vec<(f64, f64)> = taps.into_iter().filter(|t| t.1 >= 0.005).collect();
    if taps.is_empty() {
        return None;
    }
    let sum: f64 = taps.iter().map(|t| t.1).sum();
    let delays: Vec<String> = taps.iter().map(|t| f(t.0.clamp(1.0, 90000.0))).collect();
    let decays: Vec<String> = taps.iter().map(|t| f(t.1.min(1.0))).collect();
    Some(format!("aecho=in_gain=1:out_gain={}:delays={}:decays={}", f((1.0 / (1.0 + 0.5 * sum)).clamp(0.3, 1.0)), delays.join("|"), decays.join("|")))
}

fn echo(v: &Vals) -> Option<String> {
    let (d, fb, mix) = (v.n("delay"), v.n("feedback"), v.n("mix"));
    echo_taps((0..v.i("repeats")).map(|k| (d * (k + 1) as f64, mix * fb.powi(k as i32))))
}

/// Delays of the reverb's echoes, as multiples of the room's first reflection: not multiples of each other,
/// so they do not ring at one pitch.
const ROOM: [f64; 12] = [1.0, 1.31, 1.73, 2.11, 2.63, 3.07, 3.59, 4.21, 4.97, 5.83, 6.71, 7.9];

fn reverb(v: &Vals) -> Option<String> {
    let base = 10.0 + 50.0 * v.n("size");
    let fall = 0.25 + 0.7 * v.n("decay");
    let mix = v.n("mix");
    echo_taps(ROOM.iter().map(|t| (base * t, mix * 0.7 * fall.powf((t - 1.0) * 1.2))))
}

fn pitch(v: &Vals) -> Option<String> {
    let st = v.n("semitones");
    if st.abs() < 0.01 {
        return None;
    }
    // play faster / slower (pitch moves), then stretch back to the old length
    let rate = (48000.0 * 2f64.powf(st / 12.0)).round();
    Some(format!("aresample=48000,asetrate={rate},aresample=48000,atempo={}", f6(48000.0 / rate)))
}

fn stereo(v: &Vals) -> Option<String> {
    let mut p = Vec::new();
    if !near(v.n("width"), 1.0) {
        p.push(format!("extrastereo=m={}", f(v.n("width"))));
    }
    if !near(v.n("balance"), 0.0) {
        p.push(format!("stereotools=balance_out={}", f(v.n("balance"))));
    }
    join(p)
}

fn channels(v: &Vals) -> Option<String> {
    let pan = match v.i("mode") {
        0 => "c0=0.5*c0+0.5*c1|c1=0.5*c0+0.5*c1",
        1 => "c0=c0|c1=c0",
        2 => "c0=c1|c1=c1",
        _ => "c0=c1|c1=c0",
    };
    Some(format!("aformat=channel_layouts=stereo,pan=stereo|{pan}"))
}

fn modulation(v: &Vals) -> Option<String> {
    let (rate, d) = (v.n("rate"), v.n("depth"));
    Some(match v.i("kind") {
        0 => format!(
            "chorus=in_gain=0.6:out_gain=0.75:delays=40|55:decays=0.35|0.3:speeds={}|{}:depths={}|{}",
            f(rate),
            f(rate * 1.31),
            f(0.5 + 3.5 * d),
            f(0.5 + 2.5 * d)
        ),
        1 => format!("flanger=delay=1:depth={}:regen=40:width=70:speed={}", f((10.0 * d).max(0.1)), f(rate.clamp(0.1, 10.0))),
        2 => format!("aphaser=delay=3:decay={}:speed={}", f(0.2 + 0.7 * d), f(rate.clamp(0.1, 2.0))),
        3 => format!("tremolo=f={}:d={}", f(rate), f(d.clamp(0.01, 1.0))),
        _ => format!("vibrato=f={}:d={}", f(rate), f(d.clamp(0.01, 1.0))),
    })
}

fn dehum(v: &Vals) -> Option<String> {
    let base = if v.i("mains") == 0 { 50.0 } else { 60.0 };
    join((1..=v.i("harmonics")).map(|k| format!("bandreject=f={}:t=q:w={}", f(base * k as f64), f(v.n("q")))).collect())
}

fn lofi(v: &Vals) -> Option<String> {
    Some(format!("acrusher=bits={}:samples={}:mix={}", f(v.n("bits")), f(v.n("samples")), f(v.n("mix"))))
}

// ------------------------------------------------------------------ video

fn levels(v: &Vals) -> Option<String> {
    let (ib, iw, ob, ow) = (v.n("in_black"), v.n("in_white"), v.n("out_black"), v.n("out_white"));
    if near(ib, 0.0) && near(iw, 1.0) && near(ob, 0.0) && near(ow, 1.0) {
        return None;
    }
    let mut o = Vec::new();
    for (suffix, val) in [("imin", ib), ("imax", iw), ("omin", ob), ("omax", ow)] {
        for c in ["r", "g", "b"] {
            o.push(format!("{c}{suffix}={}", f(val)));
        }
    }
    Some(format!("colorlevels={}", o.join(":")))
}

fn exposure(v: &Vals) -> Option<String> {
    let (e, b) = (v.n("exposure"), v.n("black"));
    (!near(e, 0.0) || !near(b, 0.0)).then(|| format!("exposure=exposure={}:black={}", f(e), f(b)))
}

fn temperature(v: &Vals) -> Option<String> {
    let t = v.n("kelvin");
    (!near(t, 6500.0)).then(|| format!("colortemperature=temperature={}:pl={}", f(t.round()), if v.on("keep_light") { 1 } else { 0 }))
}

fn hue(v: &Vals) -> Option<String> {
    let mut p = Vec::new();
    if !near(v.n("hue"), 0.0) {
        p.push(format!("hue=h={}", f(v.n("hue"))));
    }
    if !near(v.n("vibrance"), 0.0) {
        p.push(format!("vibrance=intensity={}", f(v.n("vibrance"))));
    }
    join(p)
}

fn mono(v: &Vals) -> Option<String> {
    let s = v.n("amount");
    if s < 0.005 {
        return None;
    }
    const GREY: [f64; 3] = [0.299, 0.587, 0.114];
    let target: [[f64; 3]; 3] = match v.i("tone") {
        0 => [GREY, GREY, GREY],
        1 => [[0.393, 0.769, 0.189], [0.349, 0.686, 0.168], [0.272, 0.534, 0.131]],
        _ => [GREY.map(|c| c * 0.88), GREY.map(|c| c * 0.98), GREY.map(|c| c * 1.15)],
    };
    let mut o = Vec::new();
    for (r, row) in ["r", "g", "b"].iter().enumerate() {
        for (c, col) in ["r", "g", "b"].iter().enumerate() {
            let id = if r == c { 1.0 } else { 0.0 };
            o.push(format!("{row}{col}={}", f(id + (target[r][c] - id) * s)));
        }
    }
    Some(format!("colorchannelmixer={}", o.join(":")))
}

const LUT_INTERP: [&str; 3] = ["tetrahedral", "trilinear", "nearest"];

fn lut(v: &Vals) -> Option<String> {
    let file = v.fx.file.trim();
    if file.is_empty() {
        return None;
    }
    Some(format!("lut3d=file={}:interp={}", quote_value(&file.replace('\\', "/")), LUT_INTERP[v.i("interp").min(2)]))
}

fn blur(v: &Vals) -> Option<String> {
    (v.n("sigma") >= 0.05).then(|| format!("gblur=sigma={}", f(v.n("sigma"))))
}

fn grain(v: &Vals) -> Option<String> {
    let s = (v.n("amount") * 50.0).round() as i64;
    (s > 0).then(|| format!("noise=alls={s}:allf={}", if v.on("moving") { "t+u" } else { "u" }))
}

fn deinterlace(v: &Vals) -> Option<String> {
    let filter = if v.i("method") == 0 { "bwdif" } else { "yadif" };
    Some(format!("{filter}=mode=send_frame:deint={}", if v.on("only_interlaced") { "interlaced" } else { "all" }))
}

const EDGES: [&str; 4] = ["mirror", "blank", "original", "clamp"];

fn stabilize(v: &Vals) -> Option<String> {
    // deshake takes only multiples of 16
    let r = 16 * (v.i("range").min(3) + 1);
    Some(format!("deshake=rx={r}:ry={r}:edge={}", EDGES[v.i("edge").min(3)]))
}

fn lens(v: &Vals) -> Option<String> {
    let (k1, k2) = (v.n("k1"), v.n("k2"));
    (!near(k1, 0.0) || !near(k2, 0.0)).then(|| format!("lenscorrection=k1={}:k2={}", f(k1), f(k2)))
}

// ------------------------------------------------------------------ the table

pub static DEFS: &[Def] = &[
    // ---- audio
    Def {
        id: "limiter",
        media: Media::Audio,
        group: "Level & dynamics",
        name: "Limiter",
        desc: "Keeps peaks under a ceiling",
        keywords: "alimiter peak brickwall clip loud maximizer",
        params: &[
            num("ceiling", "Ceiling", "The level the sound never goes over", -1.0, -12.0, 0.0, " dB", 1),
            num("gain", "Input gain", "Makes the sound louder before limiting", 0.0, 0.0, 24.0, " dB", 1),
            log("release", "Release", "How fast the level comes back after a peak", 50.0, 1.0, 1000.0, " ms", 0),
        ],
        covers: &["alimiter"],
        build: limiter,
    },
    Def {
        id: "gate",
        media: Media::Audio,
        group: "Level & dynamics",
        name: "Noise gate",
        desc: "Silences the sound between words or notes",
        keywords: "agate expander background hiss breath",
        params: &[
            num("threshold", "Threshold", "Quieter than this is turned down", -40.0, -80.0, 0.0, " dB", 1),
            num("reduction", "Reduction", "How much quieter the gaps become", 24.0, 3.0, 80.0, " dB", 0),
            log("attack", "Attack", "How fast the gate opens", 10.0, 0.1, 200.0, " ms", 1),
            log("release", "Release", "How fast it closes again", 250.0, 10.0, 3000.0, " ms", 0),
        ],
        covers: &["agate"],
        build: gate,
    },
    Def {
        id: "bass_treble",
        media: Media::Audio,
        group: "Tone",
        name: "Bass & treble",
        desc: "Two tone knobs, like on a stereo",
        keywords: "bass treble low high shelf tone boost cut",
        params: &[
            num("bass", "Bass", "Boost or cut the lows", 0.0, -18.0, 18.0, " dB", 1),
            log("bass_f", "Bass frequency", "Where the lows end", 100.0, 40.0, 400.0, " Hz", 0),
            num("treble", "Treble", "Boost or cut the highs", 0.0, -18.0, 18.0, " dB", 1),
            log("treble_f", "Treble frequency", "Where the highs start", 3000.0, 1000.0, 12000.0, " Hz", 0),
        ],
        covers: &["bass", "treble"],
        build: bass_treble,
    },
    Def {
        id: "dehum",
        media: Media::Audio,
        group: "Repair",
        name: "Hum removal",
        desc: "Takes out the mains hum (50 / 60 Hz) and its overtones",
        keywords: "hum buzz mains notch bandreject ground loop 50hz 60hz",
        params: &[
            choice("mains", "Mains", "50 Hz in Europe, Asia, Africa; 60 Hz in the Americas", 0, &["50 Hz", "60 Hz"]),
            int("harmonics", "Overtones", "How many multiples of the hum frequency are removed", 4, 1, 10, ""),
            log("q", "Narrowness", "Higher = narrower cuts, less of the sound around the hum is lost", 20.0, 3.0, 60.0, "", 0),
        ],
        covers: &[],
        build: dehum,
    },
    Def {
        id: "echo",
        media: Media::Audio,
        group: "Space",
        name: "Echo",
        desc: "Repeats of the sound that fade away",
        keywords: "aecho delay repeat slapback",
        params: &[
            log("delay", "Delay", "Time between the repeats", 350.0, 20.0, 2000.0, " ms", 0),
            pct("feedback", "Feedback", "How loud each repeat is compared to the one before", 0.45),
            int("repeats", "Repeats", "", 4, 1, 8, ""),
            pct("mix", "Level", "How loud the first repeat is", 0.5),
        ],
        covers: &["aecho"],
        build: echo,
    },
    Def {
        id: "reverb",
        media: Media::Audio,
        group: "Space",
        name: "Reverb",
        desc: "A room around the sound, made of many short echoes",
        keywords: "room hall ambience space aecho",
        params: &[
            pct("size", "Room size", "Small room … large hall", 0.4),
            pct("decay", "Decay", "How long the room keeps sounding", 0.5),
            pct("mix", "Level", "How loud the room is", 0.35),
        ],
        covers: &[],
        build: reverb,
    },
    Def {
        id: "stereo",
        media: Media::Audio,
        group: "Space",
        name: "Stereo width & balance",
        desc: "Wider or narrower stereo, left / right balance",
        keywords: "extrastereo stereotools balance pan wide narrow mono",
        params: &[
            num("width", "Width", "0 = mono, 1 = unchanged, above 1 = wider", 1.0, 0.0, 2.5, "", 2),
            num("balance", "Balance", "-1 = left only, 1 = right only", 0.0, -1.0, 1.0, "", 2),
        ],
        covers: &["extrastereo"],
        build: stereo,
    },
    Def {
        id: "channels",
        media: Media::Audio,
        group: "Space",
        name: "Channels",
        desc: "Mono mix, one side on both, or swap left and right",
        keywords: "pan mono stereo left right swap one side channel",
        params: &[choice(
            "mode",
            "Do",
            "The track comes out as stereo",
            0,
            &["Mix to mono", "Left on both sides", "Right on both sides", "Swap left and right"],
        )],
        covers: &[],
        build: channels,
    },
    Def {
        id: "pitch",
        media: Media::Audio,
        group: "Character",
        name: "Pitch shift",
        desc: "Higher or lower voice, same speed",
        keywords: "pitch semitone transpose chipmunk deep voice rubberband",
        params: &[num("semitones", "Pitch", "12 semitones = one octave", 0.0, -12.0, 12.0, " st", 1)],
        covers: &["rubberband"],
        build: pitch,
    },
    Def {
        id: "modulation",
        media: Media::Audio,
        group: "Character",
        name: "Modulation",
        desc: "Chorus, flanger, phaser, tremolo or vibrato",
        keywords: "chorus flanger aphaser phaser tremolo vibrato wobble lfo",
        params: &[
            choice("kind", "Type", "", 0, &["Chorus", "Flanger", "Phaser", "Tremolo", "Vibrato"]),
            log("rate", "Rate", "Speed of the movement (the phaser goes up to 2 Hz)", 0.8, 0.1, 10.0, " Hz", 2),
            pct("depth", "Depth", "How strong the movement is", 0.5),
        ],
        covers: &["chorus", "flanger", "aphaser", "tremolo", "vibrato"],
        build: modulation,
    },
    Def {
        id: "lofi",
        media: Media::Audio,
        group: "Character",
        name: "Lo-fi",
        desc: "Bit crusher: fewer bits and samples",
        keywords: "acrusher bitcrusher crush 8bit retro distortion",
        params: &[
            num("bits", "Bits", "Fewer = rougher", 8.0, 1.0, 16.0, "", 1),
            log("samples", "Sample reduction", "Higher = more aliasing", 4.0, 1.0, 50.0, "×", 1),
            pct("mix", "Mix", "", 1.0),
        ],
        covers: &["acrusher"],
        build: lofi,
    },
    // ---- video
    Def {
        id: "levels",
        media: Media::Video,
        group: "Colour",
        name: "Levels",
        desc: "Black and white points",
        keywords: "colorlevels black point white point contrast range",
        params: &[
            pct("in_black", "Input black", "This and darker becomes black", 0.0),
            pct("in_white", "Input white", "This and brighter becomes white", 1.0),
            pct("out_black", "Output black", "How dark black is", 0.0),
            pct("out_white", "Output white", "How bright white is", 1.0),
        ],
        covers: &["colorlevels"],
        build: levels,
    },
    Def {
        id: "exposure",
        media: Media::Video,
        group: "Colour",
        name: "Exposure",
        desc: "Brighter or darker in camera stops",
        keywords: "exposure stops ev black level",
        params: &[
            num("exposure", "Exposure", "One stop doubles the light", 0.0, -3.0, 3.0, " EV", 2),
            num("black", "Black level", "Lifts or crushes the shadows", 0.0, -0.5, 0.5, "", 3),
        ],
        covers: &["exposure"],
        build: exposure,
    },
    Def {
        id: "temperature",
        media: Media::Video,
        group: "Colour",
        name: "Colour temperature",
        desc: "Warmer or cooler light",
        keywords: "colortemperature white balance warm cool kelvin",
        params: &[
            log("kelvin", "Temperature", "6500 K = unchanged; lower = warmer (orange), higher = cooler (blue)", 6500.0, 2000.0, 15000.0, " K", 0),
            toggle("keep_light", "Keep brightness", "Changes only the colour, not how bright the picture is", true),
        ],
        covers: &["colortemperature"],
        build: temperature,
    },
    Def {
        id: "hue",
        media: Media::Video,
        group: "Colour",
        name: "Hue & vibrance",
        desc: "Turn the colours, boost the dull ones",
        keywords: "hue vibrance colour shift rotate saturation",
        params: &[
            num("hue", "Hue", "Turns every colour around the colour wheel", 0.0, -180.0, 180.0, "°", 0),
            num("vibrance", "Vibrance", "Boosts the weak colours more than the strong ones", 0.0, -1.0, 1.0, "", 2),
        ],
        covers: &["hue", "vibrance"],
        build: hue,
    },
    Def {
        id: "mono",
        media: Media::Video,
        group: "Colour",
        name: "Black & white",
        desc: "Grey, sepia or cool tone",
        keywords: "monochrome grayscale greyscale sepia colorchannelmixer desaturate",
        params: &[choice("tone", "Tone", "", 0, &["Neutral", "Sepia", "Cool"]), pct("amount", "Amount", "", 1.0)],
        covers: &[],
        build: mono,
    },
    Def {
        id: "lut",
        media: Media::Video,
        group: "Colour",
        name: "LUT",
        desc: "A colour look from a .cube / .3dl file",
        keywords: "lut3d lut cube 3dl look grade film",
        params: &[
            Param {
                key: "file",
                label: "File",
                tip: "A 3D LUT: .cube, .3dl, .dat, .m3d or .csp",
                default: 0.0,
                ctl: Ctl::File(&["cube", "3dl", "dat", "m3d", "csp"]),
            },
            choice("interp", "Interpolation", "How colours between the LUT's points are found", 0, &["Tetrahedral (best)", "Trilinear", "Nearest"]),
        ],
        covers: &["lut3d"],
        build: lut,
    },
    Def {
        id: "blur",
        media: Media::Video,
        group: "Style",
        name: "Blur",
        desc: "Softens the whole picture",
        keywords: "gblur gaussian boxblur avgblur soft",
        params: &[num("sigma", "Strength", "", 4.0, 0.0, 30.0, "", 1)],
        covers: &["gblur", "boxblur", "avgblur"],
        build: blur,
    },
    Def {
        id: "grain",
        media: Media::Video,
        group: "Style",
        name: "Film grain",
        desc: "Adds fine noise",
        keywords: "noise grain film analog",
        params: &[pct("amount", "Amount", "", 0.3), toggle("moving", "Moving", "New grain on every frame, like film", true)],
        covers: &["noise"],
        build: grain,
    },
    Def {
        id: "deinterlace",
        media: Media::Video,
        group: "Restoration",
        name: "Deinterlace",
        desc: "Removes the comb lines of TV / camcorder video",
        keywords: "yadif bwdif interlaced comb lines tv",
        params: &[
            choice("method", "Method", "", 0, &["Bwdif (sharper)", "Yadif"]),
            toggle("only_interlaced", "Only interlaced frames", "Leave frames that are marked progressive as they are", true),
        ],
        covers: &["yadif", "bwdif"],
        build: deinterlace,
    },
    Def {
        id: "stabilize",
        media: Media::Video,
        group: "Restoration",
        name: "Stabilize",
        desc: "Steadies a shaky camera",
        keywords: "deshake shake stabilise stabilization steady",
        params: &[
            choice("range", "Search range", "The largest shake it follows; more is slower", 1, &["16 px", "32 px", "48 px", "64 px"]),
            choice("edge", "Edges", "What fills the border the movement uncovers", 0, &["Mirror", "Black", "Original", "Stretch edge"]),
        ],
        covers: &["deshake"],
        build: stabilize,
    },
    Def {
        id: "lens",
        media: Media::Video,
        group: "Restoration",
        name: "Lens correction",
        desc: "Straightens the curved lines of a wide lens",
        keywords: "lenscorrection fisheye barrel pincushion distortion gopro",
        params: &[
            num("k1", "Strength", "Try small values either way until straight lines look straight", 0.0, -1.0, 1.0, "", 3),
            num("k2", "Edges", "Extra correction near the corners", 0.0, -1.0, 1.0, "", 3),
        ],
        covers: &["lenscorrection"],
        build: lens,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn with(id: &str, vals: &[(&str, f64)]) -> BuiltinFx {
        let mut fx = def(id).unwrap().new_fx();
        for (k, v) in vals {
            fx.set(k, *v);
        }
        fx
    }

    #[test]
    fn the_table_is_consistent() {
        let mut ids = std::collections::HashSet::new();
        for d in DEFS {
            assert!(ids.insert(d.id), "two effects are called {}", d.id);
            assert!(crate::filters::valid_name(d.id));
            let mut keys = std::collections::HashSet::new();
            for p in d.params {
                assert!(keys.insert(p.key), "{}: two controls are called {}", d.id, p.key);
                assert_eq!(p.clamp(p.default), p.default, "{}.{}: the default is out of range", d.id, p.key);
            }
            for c in d.covers {
                assert!(crate::filters::valid_name(c));
            }
        }
    }

    #[test]
    fn values_are_kept_in_range_and_missing_ones_are_the_default() {
        let d = def("echo").unwrap();
        let mut fx = BuiltinFx { id: "echo".into(), ..Default::default() };
        assert_eq!(d.value(&fx, "delay"), 350.0);
        fx.set("delay", 1e9);
        fx.set("repeats", 2.6);
        assert_eq!((d.value(&fx, "delay"), d.value(&fx, "repeats")), (2000.0, 3.0));
        fx.set("delay", f64::NAN);
        assert_eq!(d.value(&fx, "delay"), 350.0);
    }

    #[test]
    fn neutral_settings_add_nothing() {
        for id in ["bass_treble", "stereo", "pitch", "levels", "exposure", "temperature", "hue", "lens", "lut"] {
            assert_eq!(filter_text(&def(id).unwrap().new_fx(), def(id).unwrap().media), None, "{id}");
        }
        assert_eq!(filter_text(&with("echo", &[("mix", 0.0)]), Media::Audio), None);
    }

    #[test]
    fn unknown_effects_and_the_wrong_media_add_nothing() {
        assert_eq!(filter_text(&BuiltinFx { id: "nope".into(), ..Default::default() }, Media::Audio), None);
        assert_eq!(filter_text(&def("limiter").unwrap().new_fx(), Media::Video), None);
    }

    #[test]
    fn filter_texts_are_pinned() {
        let a = |id: &str, v: &[(&str, f64)]| filter_text(&with(id, v), Media::Audio).unwrap();
        let v = |id: &str, vals: &[(&str, f64)]| filter_text(&with(id, vals), Media::Video).unwrap();
        assert_eq!(a("limiter", &[]), "alimiter=level_in=1:limit=0.891251:attack=5:release=50:level=0");
        assert_eq!(a("echo", &[("repeats", 2.0), ("feedback", 0.5), ("mix", 0.4)]), "aecho=in_gain=1:out_gain=0.7692:delays=350|700:decays=0.4|0.2");
        assert_eq!(a("pitch", &[("semitones", 12.0)]), "aresample=48000,asetrate=96000,aresample=48000,atempo=0.5");
        assert_eq!(a("bass_treble", &[("treble", -3.0)]), "treble=g=-3:f=3000");
        assert_eq!(a("dehum", &[("harmonics", 2.0), ("mains", 1.0)]), "bandreject=f=60:t=q:w=20,bandreject=f=120:t=q:w=20");
        assert_eq!(a("channels", &[("mode", 3.0)]), "aformat=channel_layouts=stereo,pan=stereo|c0=c1|c1=c0");
        assert_eq!(a("modulation", &[("kind", 3.0), ("rate", 5.0)]), "tremolo=f=5:d=0.5");
        assert_eq!(
            v("levels", &[("in_black", 0.1)]),
            "colorlevels=rimin=0.1:gimin=0.1:bimin=0.1:rimax=1:gimax=1:bimax=1:romin=0:gomin=0:bomin=0:romax=1:gomax=1:bomax=1"
        );
        assert_eq!(v("temperature", &[("kelvin", 4000.0)]), "colortemperature=temperature=4000:pl=1");
        assert_eq!(v("grain", &[]), "noise=alls=15:allf=t+u");
        assert_eq!(v("deinterlace", &[]), "bwdif=mode=send_frame:deint=interlaced");
        assert!(v("mono", &[]).starts_with("colorchannelmixer=rr=0.299:rg=0.587:rb=0.114:gr=0.299"));
        let mut l = with("lut", &[]);
        l.file = r"C:\looks\it's.cube".into();
        assert_eq!(filter_text(&l, Media::Video).unwrap(), r"lut3d=file='C\:/looks/it'\\\''s.cube':interp=tetrahedral");
    }

    #[test]
    fn saved_effects_read_back() {
        use crate::{Effect, EffectKind};
        let e = Effect::new(EffectKind::Builtin(BuiltinFx { id: "echo".into(), vals: vec![("delay".into(), 500.0)], ..Default::default() }));
        let j = serde_json::to_string(&e).unwrap();
        assert_eq!(j, r#"{"on":true,"kind":{"Builtin":{"id":"echo","vals":[["delay",500.0]]}}}"#);
        assert_eq!(serde_json::from_str::<Effect>(&j).unwrap(), e);
        let old: BuiltinFx = serde_json::from_str(r#"{"id":"lut"}"#).unwrap();
        assert_eq!((old.vals.len(), old.file.as_str()), (0, ""));
    }
}
