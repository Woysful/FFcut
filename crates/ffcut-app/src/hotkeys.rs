//! Keyboard shortcuts: the list of actions, their default keys, and the text form kept in settings.json
//! (`"Ctrl+Shift+Z"`). Only what the user changed is saved; everything else follows the defaults.

use eframe::egui::{Key, Modifiers};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    PlayPause,
    StepBack,
    StepForward,
    SeekBack,
    SeekForward,
    TrimStart,
    TrimEnd,
    ResetTrim,
    Crop,
    Loop,
    LockTrim,
    Undo,
    Redo,
    Open,
    Export,
    Fullscreen,
    VideoFullscreen,
}

impl Action {
    pub const ALL: [Action; 17] = [
        Action::PlayPause,
        Action::StepBack,
        Action::StepForward,
        Action::SeekBack,
        Action::SeekForward,
        Action::TrimStart,
        Action::TrimEnd,
        Action::ResetTrim,
        Action::Loop,
        Action::LockTrim,
        Action::Crop,
        Action::Undo,
        Action::Redo,
        Action::Open,
        Action::Export,
        Action::Fullscreen,
        Action::VideoFullscreen,
    ];

    /// The name under which it is saved.
    pub fn id(self) -> &'static str {
        match self {
            Action::PlayPause => "play_pause",
            Action::StepBack => "step_back",
            Action::StepForward => "step_forward",
            Action::SeekBack => "seek_back",
            Action::SeekForward => "seek_forward",
            Action::TrimStart => "trim_start",
            Action::TrimEnd => "trim_end",
            Action::ResetTrim => "reset_trim",
            Action::Crop => "crop",
            Action::Loop => "loop",
            Action::LockTrim => "lock_trim",
            Action::Undo => "undo",
            Action::Redo => "redo",
            Action::Open => "open",
            Action::Export => "export",
            Action::Fullscreen => "fullscreen",
            Action::VideoFullscreen => "video_fullscreen",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Action::PlayPause => "Play / pause",
            Action::StepBack => "Step back (frame / 1 s)",
            Action::StepForward => "Step forward (frame / 1 s)",
            Action::SeekBack => "Seek back 5 s",
            Action::SeekForward => "Seek forward 5 s",
            Action::TrimStart => "Trim start = playhead",
            Action::TrimEnd => "Trim end = playhead",
            Action::ResetTrim => "Reset trim",
            Action::Crop => "Toggle crop",
            Action::Loop => "Loop the selection",
            Action::LockTrim => "Lock trim handles",
            Action::Undo => "Undo",
            Action::Redo => "Redo",
            Action::Open => "Open a file",
            Action::Export => "Export",
            Action::Fullscreen => "Interface fullscreen",
            Action::VideoFullscreen => "Video fullscreen",
        }
    }

    /// Global shortcuts also work while a text field has the focus.
    pub fn global(self) -> bool {
        matches!(self, Action::Open | Action::Export | Action::Fullscreen | Action::VideoFullscreen)
    }

    pub fn defaults(self) -> &'static [&'static str] {
        match self {
            Action::PlayPause => &["Space"],
            Action::StepBack => &["Left", "Comma"],
            Action::StepForward => &["Right", "Period"],
            Action::SeekBack => &["Shift+Left"],
            Action::SeekForward => &["Shift+Right"],
            Action::TrimStart => &["Q"],
            Action::TrimEnd => &["E"],
            Action::ResetTrim => &["R"],
            Action::Crop => &["C"],
            Action::Loop => &["A"],
            Action::LockTrim => &["Shift+A"],
            Action::Undo => &["Ctrl+Z"],
            Action::Redo => &["Ctrl+Shift+Z", "Ctrl+Y"],
            Action::Open => &["Ctrl+O", "Ctrl+N"],
            Action::Export => &["Ctrl+E", "Ctrl+S"],
            Action::Fullscreen => &["F11"],
            Action::VideoFullscreen => &["Ctrl+F"],
        }
    }
}

/// One key with its modifiers. `ctrl` means Ctrl, or Cmd on a Mac.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Chord {
    pub fn new(key: Key, m: Modifiers) -> Self {
        Chord { key, ctrl: m.command || m.ctrl, shift: m.shift, alt: m.alt }
    }

    pub fn modifiers(&self) -> Modifiers {
        Modifiers { alt: self.alt, ctrl: false, shift: self.shift, mac_cmd: false, command: self.ctrl }
    }

    /// How many modifiers it needs: the more specific one has to be tried first.
    pub fn weight(&self) -> u8 {
        self.ctrl as u8 + self.shift as u8 + self.alt as u8
    }

    /// The saved form: `Ctrl+Shift+Left`.
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        s.push_str(self.key.name());
        s
    }

    pub fn parse(text: &str) -> Option<Chord> {
        let mut c = Chord { key: Key::A, ctrl: false, shift: false, alt: false };
        let mut parts: Vec<&str> = text.split('+').collect();
        // a "+" key itself ends with an empty part
        let key = parts.pop().filter(|k| !k.is_empty()).map(str::to_string).or_else(|| text.ends_with('+').then(|| "+".to_string()))?;
        for p in parts.into_iter().filter(|p| !p.is_empty()) {
            match p.to_ascii_lowercase().as_str() {
                "ctrl" | "cmd" | "command" => c.ctrl = true,
                "shift" => c.shift = true,
                "alt" => c.alt = true,
                _ => return None,
            }
        }
        c.key = Key::from_name(&key)?;
        Some(c)
    }

    /// For the screen: `Ctrl+Shift+←`.
    pub fn display(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        s.push_str(match self.key {
            Key::ArrowLeft => "←",
            Key::ArrowRight => "→",
            Key::ArrowUp => "↑",
            Key::ArrowDown => "↓",
            k => k.symbol_or_name(),
        });
        s
    }
}

/// Ctrl, Shift, Alt and Super arrive as keys of their own; alone they are not a shortcut.
pub fn is_modifier(k: Key) -> bool {
    matches!(k, Key::ShiftLeft | Key::ShiftRight | Key::ControlLeft | Key::ControlRight | Key::AltLeft | Key::AltRight | Key::SuperLeft | Key::SuperRight)
}

/// The user's changes: action id → chords in text form (an empty list = no key).
pub type Overrides = BTreeMap<String, Vec<String>>;

/// The keys of an action: the user's, or the defaults.
pub fn chords(ov: &Overrides, a: Action) -> Vec<Chord> {
    match ov.get(a.id()) {
        Some(list) => list.iter().filter_map(|t| Chord::parse(t)).collect(),
        None => a.defaults().iter().filter_map(|t| Chord::parse(t)).collect(),
    }
}

/// Text for a tooltip or menu: the first key of the action, or empty.
pub fn hint(ov: &Overrides, a: Action) -> String {
    chords(ov, a).first().map(|c| c.display()).unwrap_or_default()
}

/// `"Play / pause  (Space)"`, or just the label when the action has no key.
pub fn with_hint(ov: &Overrides, label: &str, a: Action) -> String {
    let h = hint(ov, a);
    if h.is_empty() { label.to_string() } else { format!("{label}  ({h})") }
}

/// Give `chord` to `a` as its `slot`-th key (None = remove that slot). A chord another action had is taken from it.
/// Returns the action that lost its key, if any.
pub fn assign(ov: &mut Overrides, a: Action, slot: usize, chord: Option<Chord>) -> Option<Action> {
    let mut taken = None;
    if let Some(c) = chord {
        for other in Action::ALL.into_iter().filter(|o| *o != a) {
            let mut list = chords(ov, other);
            if let Some(p) = list.iter().position(|x| *x == c) {
                list.remove(p);
                ov.insert(other.id().to_string(), list.iter().map(Chord::to_text).collect());
                taken = Some(other);
            }
        }
    }
    let mut mine = chords(ov, a);
    match chord {
        Some(c) => {
            let old = mine.get(slot).copied();
            mine.retain(|x| *x != c);
            match old.and_then(|o| mine.iter().position(|x| *x == o)) {
                Some(p) => mine[p] = c,
                None => mine.push(c),
            }
            mine.truncate(2);
        }
        None => {
            if slot < mine.len() {
                mine.remove(slot);
            }
        }
    }
    let text: Vec<String> = mine.iter().map(Chord::to_text).collect();
    let defaults: Vec<String> = a.defaults().iter().map(|s| s.to_string()).collect();
    if text == defaults {
        ov.remove(a.id());
    } else {
        ov.insert(a.id().to_string(), text);
    }
    taken
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_parses_and_round_trips() {
        for a in Action::ALL {
            for d in a.defaults() {
                let c = Chord::parse(d).unwrap_or_else(|| panic!("{d} does not parse"));
                assert_eq!(&c.to_text(), d);
            }
        }
    }

    #[test]
    fn defaults_do_not_collide() {
        let ov = Overrides::new();
        let mut seen: Vec<(Chord, Action)> = Vec::new();
        for a in Action::ALL {
            for c in chords(&ov, a) {
                assert!(!seen.iter().any(|(x, _)| *x == c), "{} is used twice", c.display());
                seen.push((c, a));
            }
        }
    }

    #[test]
    fn assigning_a_taken_key_moves_it() {
        let mut ov = Overrides::new();
        let c = Chord::parse("Q").unwrap();
        let lost = assign(&mut ov, Action::Crop, 0, Some(c));
        assert_eq!(lost, Some(Action::TrimStart));
        assert_eq!(chords(&ov, Action::Crop), vec![c]);
        assert!(chords(&ov, Action::TrimStart).is_empty());
        // putting the defaults back leaves no override behind
        assign(&mut ov, Action::Crop, 0, Chord::parse("C"));
        assert!(!ov.contains_key("crop"));
    }

    #[test]
    fn second_slot_and_clearing() {
        let mut ov = Overrides::new();
        assign(&mut ov, Action::PlayPause, 1, Chord::parse("K"));
        assert_eq!(chords(&ov, Action::PlayPause).len(), 2);
        assign(&mut ov, Action::PlayPause, 0, None);
        assert_eq!(chords(&ov, Action::PlayPause), vec![Chord::parse("K").unwrap()]);
    }

    #[test]
    fn hints() {
        let ov = Overrides::new();
        assert_eq!(hint(&ov, Action::Crop), "C");
        assert_eq!(with_hint(&ov, "Crop", Action::Crop), "Crop  (C)");
        assert_eq!(hint(&ov, Action::Undo), "Ctrl+Z");
    }
}
