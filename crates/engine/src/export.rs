//! Offline loop export (M3): render a [`Project`] for a whole number of bars into memory or a
//! 16-bit stereo WAV file.
//!
//! This is UI-thread code: it builds its own [`Engine`] with [`new_engine`], so it never
//! touches the live audio stream, and it is free to allocate.
//!
//! Length formula (frames, per channel):
//!
//! ```text
//! samples_per_step = sample_rate * 60 / bpm / 4          (f64, see Transport::samples_per_step)
//! bar_frames       = round(16 * samples_per_step)
//! total_frames     = repeats * bar_frames + round(TAIL_SECONDS * sample_rate)
//! ```
//!
//! At 120 BPM / 48 kHz one bar is exactly 96 000 frames, so `render_loop(p, 1, 48_000.0)` is
//! `(96_000 + 96_000) * 2` interleaved samples.

use std::fmt;
use std::path::Path;

use crate::engine::{Engine, new_engine};
use crate::project::Project;
use crate::transport::{STEPS_PER_BAR, Transport};

/// Seconds rendered after the last bar with the transport stopped, so delay and reverb tails
/// decay instead of being cut.
pub const TAIL_SECONDS: f32 = 2.0;

/// Frames per `render` call, matching a typical device block size.
pub const BLOCK_FRAMES: usize = 512;

/// Largest number of loop repeats accepted. The UI offers 1, 2, 4 and 8; any `1..=8` works.
pub const MAX_REPEATS: u32 = 8;

/// Bits per sample of the exported WAV.
pub const WAV_BITS: u16 = 16;

/// Summary of a finished export.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExportStats {
    /// Frames (stereo sample pairs) written, loop plus tail.
    pub frames: usize,
    /// `frames / sample_rate`.
    pub duration_seconds: f32,
    /// Largest absolute sample value before 16-bit conversion (`0.0` for silence).
    pub peak: f32,
}

/// Why an export failed.
#[derive(Debug)]
pub enum ExportError {
    /// Creating or writing the file failed.
    Io(std::io::Error),
    /// The WAV encoder rejected the data or format.
    Hound(hound::Error),
    /// `repeats` was outside `1..=MAX_REPEATS`.
    InvalidRepeats(u32),
    /// The sample rate was not a finite positive number.
    InvalidSampleRate(f32),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportError::Io(e) => write!(f, "WAV file I/O error: {e}"),
            ExportError::Hound(e) => write!(f, "WAV encoding error: {e}"),
            ExportError::InvalidRepeats(n) => {
                write!(f, "repeats must be between 1 and {MAX_REPEATS}, got {n}")
            }
            ExportError::InvalidSampleRate(sr) => {
                write!(f, "sample rate must be finite and positive, got {sr}")
            }
        }
    }
}

impl std::error::Error for ExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ExportError::Io(e) => Some(e),
            ExportError::Hound(e) => Some(e),
            ExportError::InvalidRepeats(_) | ExportError::InvalidSampleRate(_) => None,
        }
    }
}

impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self {
        ExportError::Io(e)
    }
}

impl From<hound::Error> for ExportError {
    fn from(e: hound::Error) -> Self {
        match e {
            hound::Error::IoError(io) => ExportError::Io(io),
            other => ExportError::Hound(other),
        }
    }
}

/// Frames in one 16-step bar at `bpm` (clamped to the transport range like the engine does):
/// `round(16 * sample_rate * 60 / bpm / 4)`.
#[must_use]
pub fn bar_frames(bpm: f32, sample_rate: f32) -> usize {
    let mut transport = Transport::new();
    transport.set_bpm(bpm);
    let frames = STEPS_PER_BAR as f64 * transport.samples_per_step(sample_rate);
    frames.round().max(0.0) as usize
}

/// Frames in the decay tail: `round(TAIL_SECONDS * sample_rate)`.
#[must_use]
pub fn tail_frames(sample_rate: f32) -> usize {
    (f64::from(TAIL_SECONDS) * f64::from(sample_rate))
        .round()
        .max(0.0) as usize
}

/// Total frames [`render_loop`] produces: `repeats * bar_frames + tail_frames`.
#[must_use]
pub fn export_frames(project: &Project, repeats: u32, sample_rate: f32) -> usize {
    repeats as usize * bar_frames(project.bpm, sample_rate) + tail_frames(sample_rate)
}

/// `Ok(())` when `repeats` is in `1..=MAX_REPEATS` and `sample_rate` is finite and positive.
pub fn validate(repeats: u32, sample_rate: f32) -> Result<(), ExportError> {
    if !(1..=MAX_REPEATS).contains(&repeats) {
        return Err(ExportError::InvalidRepeats(repeats));
    }
    if !sample_rate.is_finite() || sample_rate <= 0.0 {
        return Err(ExportError::InvalidSampleRate(sample_rate));
    }
    Ok(())
}

/// Renders `repeats` bars of `project` plus a [`TAIL_SECONDS`] tail into interleaved stereo.
///
/// A fresh engine is created at `sample_rate`, the project applied, the transport started,
/// exactly `repeats * bar_frames` frames rendered in [`BLOCK_FRAMES`] blocks (last block
/// partial), then the transport stopped and the tail rendered. The result has
/// `export_frames(project, repeats, sample_rate) * 2` samples.
///
/// `repeats` is clamped into `1..=MAX_REPEATS` and a non-finite or non-positive sample rate
/// falls back to 48 kHz; use [`render_loop_wav`] (or [`validate`]) for strict checking.
#[must_use]
pub fn render_loop(project: &Project, repeats: u32, sample_rate: f32) -> Vec<f32> {
    let repeats = repeats.clamp(1, MAX_REPEATS);
    let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        48_000.0
    };
    let (_handle, mut engine) = new_engine(sample_rate);
    project.apply_to(&mut engine);

    // Use the engine's own (clamped) tempo so the loop length matches what it plays.
    let loop_frames = repeats as usize * bar_frames(engine.bpm(), sample_rate);
    let tail = tail_frames(sample_rate);
    let mut out = vec![0.0_f32; (loop_frames + tail) * 2];
    let (loop_buf, tail_buf) = out.split_at_mut(loop_frames * 2);

    engine.play();
    render_blocks(&mut engine, loop_buf);
    engine.stop();
    render_blocks(&mut engine, tail_buf);
    out
}

/// Fills `out` (interleaved stereo) in [`BLOCK_FRAMES`] blocks; the last block may be short.
fn render_blocks(engine: &mut Engine, out: &mut [f32]) {
    for block in out.chunks_mut(BLOCK_FRAMES * 2) {
        let frames = block.len() / 2;
        engine.render(block, frames);
    }
}

/// Renders like [`render_loop`] and writes a 16-bit PCM stereo WAV to `path`.
///
/// Samples are clamped to `-1.0..=1.0` before conversion (no dither). `repeats` must be in
/// `1..=MAX_REPEATS` and `sample_rate` finite and positive.
pub fn render_loop_wav(
    project: &Project,
    repeats: u32,
    sample_rate: f32,
    path: impl AsRef<Path>,
) -> Result<ExportStats, ExportError> {
    validate(repeats, sample_rate)?;
    let samples = render_loop(project, repeats, sample_rate);
    let peak = samples
        .iter()
        .map(|s| if s.is_nan() { 0.0 } else { s.abs() })
        .fold(0.0_f32, f32::max);

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sample_rate.round() as u32,
        bits_per_sample: WAV_BITS,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &s in &samples {
        writer.write_sample(to_i16(s))?;
    }
    writer.finalize()?;

    let frames = samples.len() / 2;
    Ok(ExportStats {
        frames,
        duration_seconds: frames as f32 / sample_rate,
        peak,
    })
}

/// Clamps to `-1.0..=1.0` and scales to a 16-bit sample. NaN becomes `0`.
fn to_i16(sample: f32) -> i16 {
    if !sample.is_finite() {
        return 0;
    }
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_formula_at_120_bpm_48k() {
        assert_eq!(bar_frames(120.0, 48_000.0), 96_000);
        assert_eq!(tail_frames(48_000.0), 96_000);
        let p = Project::default();
        assert_eq!(export_frames(&p, 1, 48_000.0), 192_000);
        assert_eq!(export_frames(&p, 4, 48_000.0), 4 * 96_000 + 96_000);
        // Out-of-range tempo is clamped like the engine clamps it (60..=200 BPM).
        assert_eq!(bar_frames(1.0, 48_000.0), bar_frames(60.0, 48_000.0));
        // Non-integer step lengths round the whole bar, not each step.
        // 44_100 * 60 / 130 / 4 = 5088.4615.. per step; 16 steps = 81415.38.. -> 81415.
        assert_eq!(bar_frames(130.0, 44_100.0), 81_415);
    }

    #[test]
    fn repeats_and_sample_rate_validation() {
        assert!(validate(1, 48_000.0).is_ok());
        assert!(validate(8, 44_100.0).is_ok());
        assert!(matches!(
            validate(0, 48_000.0),
            Err(ExportError::InvalidRepeats(0))
        ));
        assert!(matches!(
            validate(9, 48_000.0),
            Err(ExportError::InvalidRepeats(9))
        ));
        assert!(matches!(
            validate(1, f32::NAN),
            Err(ExportError::InvalidSampleRate(_))
        ));
        assert!(matches!(
            validate(1, 0.0),
            Err(ExportError::InvalidSampleRate(_))
        ));
        let err = ExportError::InvalidRepeats(9);
        assert!(err.to_string().contains("between 1 and 8"));
        assert!(std::error::Error::source(&err).is_none());
    }

    #[test]
    fn sample_conversion_clamps() {
        assert_eq!(to_i16(0.0), 0);
        assert_eq!(to_i16(1.0), i16::MAX);
        assert_eq!(to_i16(2.0), i16::MAX);
        assert_eq!(to_i16(-1.0), -i16::MAX);
        assert_eq!(to_i16(-5.0), -i16::MAX);
        assert_eq!(to_i16(f32::NAN), 0);
        assert_eq!(to_i16(0.5), 16_384);
    }
}
