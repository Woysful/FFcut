//! Multi-track audio preview in mpv: extra audio files + an `lavfi-complex` graph that mixes them.

use ffcut_core::preview::lavfi_mix;
use libmpv2::Mpv;

/// mpv's id of an audio file that was added with `audio-add` (matched by its path).
pub fn external_audio_id(mpv: &Mpv, path: &str) -> Option<i64> {
    let n = mpv.get_property::<i64>("track-list/count").ok()?;
    (0..n).find_map(|i| {
        let kind = mpv.get_property::<String>(&format!("track-list/{i}/type")).ok()?;
        let ext = mpv.get_property::<String>(&format!("track-list/{i}/external-filename")).ok()?;
        if kind == "audio" && ext == path {
            mpv.get_property::<i64>(&format!("track-list/{i}/id")).ok()
        } else {
            None
        }
    })
}

/// Makes sure `path` is loaded as an extra audio track and returns its id.
pub fn ensure_external(mpv: &Mpv, path: &str) -> Option<i64> {
    if let Some(id) = external_audio_id(mpv, path) {
        return Some(id);
    }
    // "auto": do not switch the selected track, the mix graph picks what it needs
    mpv.command("audio-add", &[path, "auto"]).ok()?;
    external_audio_id(mpv, path)
}

/// Plays exactly these tracks together (`(audio track id, volume)`); an empty list = silence.
pub fn set_mix(mpv: &Mpv, parts: &[(i64, f32)]) -> Result<(), String> {
    match lavfi_mix(parts) {
        Some(graph) => mpv.set_property("lavfi-complex", graph.as_str()).map_err(|e| format!("mix graph rejected: {e}")),
        None => {
            mpv.set_property("lavfi-complex", "").map_err(|e| e.to_string())?;
            mpv.set_property("aid", "no").map_err(|e| e.to_string())
        }
    }
}

/// Back to plain playback of one audio track (`aid`, 1-based).
pub fn single_track(mpv: &Mpv, aid: i64) -> Result<(), String> {
    mpv.set_property("lavfi-complex", "").map_err(|e| e.to_string())?;
    // leaving the mix graph keeps `aid` at its old value and mpv then does not reconnect the track
    // (silence until you pick another one); deselecting first forces a fresh selection
    mpv.set_property("aid", "no").map_err(|e| e.to_string())?;
    mpv.set_property("aid", aid).map_err(|e| e.to_string())
}
