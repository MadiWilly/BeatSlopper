//! Shared helpers for the offline-render integration tests.
//!
//! Everything renders at 48 kHz into plain `Vec`s; no audio device is involved.

#![allow(dead_code)]

use engine::Engine;

/// Sample rate used by all engine tests.
pub const SAMPLE_RATE: f32 = 48_000.0;

/// Converts milliseconds to a whole number of frames at [`SAMPLE_RATE`].
pub fn ms_to_frames(ms: f32) -> usize {
    (SAMPLE_RATE * ms * 0.001).round() as usize
}

/// Renders `frames` frames of interleaved stereo in a single `render` call.
pub fn render(engine: &mut Engine, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0_f32; frames * 2];
    engine.render(&mut out, frames);
    out
}

/// Renders `frames` frames in device-sized blocks of `block` frames (last block may be short).
pub fn render_in_blocks(engine: &mut Engine, frames: usize, block: usize) -> Vec<f32> {
    let mut out = vec![0.0_f32; frames * 2];
    for chunk in out.chunks_mut(block * 2) {
        let n = chunk.len() / 2;
        engine.render(chunk, n);
    }
    out
}

/// Extracts the left channel of an interleaved stereo buffer.
pub fn left(stereo: &[f32]) -> Vec<f32> {
    stereo.iter().step_by(2).copied().collect()
}

/// Extracts the right channel of an interleaved stereo buffer.
pub fn right(stereo: &[f32]) -> Vec<f32> {
    stereo.iter().skip(1).step_by(2).copied().collect()
}

/// Root-mean-square level of a buffer (0.0 for an empty buffer).
pub fn rms(buf: &[f32]) -> f32 {
    if buf.is_empty() {
        return 0.0;
    }
    let sum: f64 = buf.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    (sum / buf.len() as f64).sqrt() as f32
}

/// Largest absolute sample value (0.0 for an empty buffer). NaN counts as infinite.
pub fn peak(buf: &[f32]) -> f32 {
    buf.iter()
        .map(|&s| if s.is_nan() { f32::INFINITY } else { s.abs() })
        .fold(0.0, f32::max)
}

/// `true` when every sample is finite (no NaN, no infinity).
pub fn all_finite(buf: &[f32]) -> bool {
    buf.iter().all(|s| s.is_finite())
}

/// `true` when every sample is exactly zero.
pub fn is_silent(buf: &[f32]) -> bool {
    buf.iter().all(|&s| s == 0.0)
}

/// Estimates the fundamental frequency of a mono buffer from its positive-going
/// zero crossings, with linear interpolation of each crossing instant.
///
/// Returns `None` when fewer than two crossings are found.
pub fn estimate_pitch_hz(mono: &[f32], sample_rate: f32) -> Option<f32> {
    let mut first: Option<f64> = None;
    let mut last: Option<f64> = None;
    let mut count = 0_usize;
    for i in 1..mono.len() {
        let (a, b) = (mono[i - 1], mono[i]);
        if a < 0.0 && b >= 0.0 {
            let frac = f64::from(a) / f64::from(a - b);
            let t = (i - 1) as f64 + frac;
            if first.is_none() {
                first = Some(t);
            }
            last = Some(t);
            count += 1;
        }
    }
    match (first, last) {
        (Some(f), Some(l)) if count >= 2 && l > f => {
            Some((f64::from(sample_rate) * (count - 1) as f64 / (l - f)) as f32)
        }
        _ => None,
    }
}
