//! Numbers as they appear in ffmpeg arguments: fixed decimals, then trailing zeros dropped.

/// `v` rounded to `decimals` places, without trailing zeros: `2.5000` → `2.5`, `3.000` → `3`.
/// A value that rounds to zero is always `0`, never `-0`.
pub(crate) fn num(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { s.as_str() };
    if s.is_empty() || s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::num;

    #[test]
    fn trims_and_never_says_minus_zero() {
        assert_eq!(num(2.5, 4), "2.5");
        assert_eq!(num(3.0, 3), "3");
        assert_eq!(num(10.0, 3), "10");
        assert_eq!(num(0.0, 4), "0");
        assert_eq!(num(-0.0, 4), "0");
        assert_eq!(num(-0.00001, 4), "0");
        assert_eq!(num(-1.25, 4), "-1.25");
        assert_eq!(num(1.995262315, 6), "1.995262");
        assert_eq!(num(0.12345, 3), "0.123");
        assert_eq!(num(100.0, 0), "100");
    }
}
