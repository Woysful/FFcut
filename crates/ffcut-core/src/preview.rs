//! Live preview of several audio tracks at once: an mpv `--lavfi-complex` graph.
//!
//! Pure string building, so it can be unit-tested; `tests/preview_mix.rs` feeds it to a real libmpv.
//! The preview mixes what is *audible* with each track's volume. Trim, shift and effects are not done in
//! this graph: a track that has them is first rendered by ffmpeg with the export's own filters
//! (`audiofx::preview_render_args`) and that file takes the track's place here.

/// `parts`: (mpv audio track id, volume where 1.0 = unchanged). `None` = nothing to play.
pub fn lavfi_mix(parts: &[(i64, f32)]) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    // Every branch is brought to the same rate / layout first: amix wants uniform inputs, and tracks
    // from different files rarely agree (44.1 kHz music vs 48 kHz opus, mono vs 5.1 ...).
    let norm = "aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo";
    let mut chains: Vec<String> = parts.iter().enumerate().map(|(n, (aid, vol))| format!("[aid{aid}]{norm},volume={:.3}[p{n}]", vol.max(0.0))).collect();
    if parts.len() == 1 {
        chains.push("[p0]anull[ao]".into());
    } else {
        chains.push(crate::audiofx::amix((0..parts.len()).map(|n| format!("p{n}")), "ao"));
    }
    Some(chains.join(";"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_audible_nothing_to_play() {
        assert_eq!(lavfi_mix(&[]), None);
    }

    #[test]
    fn single_and_mixed() {
        let one = lavfi_mix(&[(2, 0.5)]).unwrap();
        assert!(one.contains("[aid2]") && one.contains("volume=0.500[p0]") && one.ends_with("[p0]anull[ao]"), "{one}");
        let two = lavfi_mix(&[(1, 1.0), (3, 2.0)]).unwrap();
        assert!(two.contains("[aid1]") && two.contains("[aid3]") && two.contains("volume=2.000[p1]"), "{two}");
        assert!(two.ends_with("[p0][p1]amix=inputs=2:duration=longest:normalize=0[ao]"), "{two}");
    }
}
