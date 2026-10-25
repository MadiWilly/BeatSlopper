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

/// RMS of the first difference `x[n] - x[n-1]`: a crude 6 dB/octave high-pass, so it grows
/// with high-frequency content. Useful as a brightness / aliasing proxy.
pub fn hp_rms(buf: &[f32]) -> f32 {
    if buf.len() < 2 {
        return 0.0;
    }
    let diff: Vec<f32> = buf.windows(2).map(|w| w[1] - w[0]).collect();
    rms(&diff)
}

/// High-pass RMS relative to the plain RMS: how bright a signal is, independent of level.
/// Returns `0.0` for a silent buffer.
pub fn brightness(buf: &[f32]) -> f32 {
    let level = rms(buf);
    if level <= 0.0 {
        0.0
    } else {
        hp_rms(buf) / level
    }
}

/// Splits `mono` into consecutive windows of `len` samples and maps each through `f`.
pub fn per_window<T>(mono: &[f32], len: usize, f: impl Fn(&[f32]) -> T) -> Vec<T> {
    mono.chunks_exact(len).map(f).collect()
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

/// Bypasses the effect chain (drive, delay and reverb mixes to zero) so DSP measurements see
/// the dry voice path. The engine's defaults are deliberately a little wet for the product.
#[allow(dead_code)]
pub fn dry_fx(engine: &mut engine::Engine) {
    engine.set_param(engine::ParamId::DriveMix, 0.0);
    engine.set_param(engine::ParamId::DelayMix, 0.0);
    engine.set_param(engine::ParamId::ReverbMix, 0.0);
}

// --- sequencer helpers (shared by tests/sequencer.rs and tests/song.rs) --------------------

/// Anything above this counts as sound in the onset helpers.
pub const ONSET_EPS: f32 = 1.0e-9;

/// A dry, click-fast sine patch so the synth lane's note boundaries are sharp and every
/// sample between two notes is exactly zero. Smoothers settle during a stopped warm-up.
pub fn seq_engine() -> (engine::EngineHandle, Engine) {
    use engine::ParamId;
    let (handle, mut engine) = engine::new_engine(SAMPLE_RATE);
    dry_fx(&mut engine);
    engine.set_param(ParamId::Osc1Wave, engine::Wave::Sine.index() as f32);
    engine.set_param(ParamId::Osc1Detune, 0.0);
    engine.set_param(ParamId::Osc1Mix, 1.0);
    engine.set_param(ParamId::Osc2Mix, 0.0);
    engine.set_param(ParamId::FilterCutoff, 20_000.0);
    engine.set_param(ParamId::FilterResonance, 0.0);
    engine.set_param(ParamId::FilterEnvAmount, 0.0);
    engine.set_param(ParamId::LfoDepth, 0.0);
    engine.set_param(ParamId::Glide, 0.0);
    engine.set_param(ParamId::AmpAttack, 0.001);
    engine.set_param(ParamId::AmpDecay, 0.001);
    engine.set_param(ParamId::AmpSustain, 1.0);
    engine.set_param(ParamId::AmpRelease, 0.001);
    let warmup = render(&mut engine, ms_to_frames(200.0));
    assert!(is_silent(&warmup), "a stopped engine must be silent");
    (handle, engine)
}

/// Puts `note` on every step of the edit pattern's first bar whose index satisfies `keep`.
pub fn lane(engine: &mut Engine, note: u8, keep: impl Fn(usize) -> bool) {
    for step in 0..engine::STEPS {
        let n = keep(step).then_some(note);
        engine.set_synth_step(step as u8, n, false);
    }
}

/// Sample indices where the signal becomes audible after at least `QUIET` silent samples
/// (or from the very first sample).
pub fn onsets(mono: &[f32]) -> Vec<usize> {
    const QUIET: usize = 8;
    let mut out = Vec::new();
    let mut silent_run = QUIET; // the signal before the buffer counts as silence
    for (i, &x) in mono.iter().enumerate() {
        if x.abs() > ONSET_EPS {
            if silent_run >= QUIET {
                out.push(i);
            }
            silent_run = 0;
        } else {
            silent_run += 1;
        }
    }
    out
}

/// Asserts every onset lands within one sample of its expected trigger.
pub fn assert_onsets(found: &[usize], expected: &[usize]) {
    assert_eq!(
        found.len(),
        expected.len(),
        "onsets {found:?}, expected {expected:?}"
    );
    for (f, e) in found.iter().zip(expected) {
        assert!(
            f.abs_diff(*e) <= 1,
            "onset at {f}, expected {e} (+-1); all onsets {found:?}"
        );
    }
}

/// Starts the transport and renders `frames` frames, returning the left channel.
pub fn play_and_render(engine: &mut Engine, frames: usize) -> Vec<f32> {
    engine.play();
    left(&render(engine, frames))
}
