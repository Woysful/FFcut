//! A small, stable hash for cache keys (FNV-1a, 64 bit).
//!
//! `std`'s `DefaultHasher` may change between Rust releases, and the keys of loudness measurements are
//! stored together with the track, so they must come out the same in every build.

use serde::Serialize;
use std::hash::Hasher;

pub(crate) struct Fnv(u64);

impl Fnv {
    pub(crate) fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }

    /// Feeds the JSON form of `v` (exact for floats, stable field order).
    pub(crate) fn json<T: Serialize + ?Sized>(&mut self, v: &T) {
        // Writing into the hasher cannot fail, and none of our types has map keys that are not strings.
        let _ = serde_json::to_writer(&mut *self, v);
        self.write_u8(0xff);
    }

    /// Feeds a time rounded to the millisecond: closer values are the same edit.
    pub(crate) fn millis(&mut self, secs: f64) {
        self.write_i64((secs * 1000.0).round() as i64);
    }
}

impl Hasher for Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= *b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

impl std::io::Write for Fnv {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Hasher::write(self, buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_stable_and_tells_values_apart() {
        let h = |v: &(&str, f64)| {
            let mut f = Fnv::new();
            f.json(v);
            f.finish()
        };
        // FNV-1a of known input: this value must never change
        let mut f = Fnv::new();
        Hasher::write(&mut f, b"a");
        assert_eq!(f.finish(), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(h(&("x", 1.0)), h(&("x", 1.0)));
        assert_ne!(h(&("x", 1.0)), h(&("x", 1.0000001)));
    }
}
