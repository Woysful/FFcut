//! A ready-made effect with its own controls (echo, limiter, LUT...), made of one or more ffmpeg filters.
//! `builtin` says what each one is and turns this into filter text.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BuiltinFx {
    /// Which effect (`echo`, `lut`...), see `builtin::DEFS`.
    pub id: String,
    /// The value of each control, by key; a missing control has its default (a toggle is 0 / 1, a choice its
    /// position in the list).
    #[serde(default)]
    pub vals: Vec<(String, f64)>,
    /// The file the effect reads (a LUT), empty = none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
}

impl BuiltinFx {
    pub fn get(&self, key: &str) -> Option<f64> {
        self.vals.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
    }
    pub fn set(&mut self, key: &str, v: f64) {
        match self.vals.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = v,
            None => self.vals.push((key.to_string(), v)),
        }
    }
}
