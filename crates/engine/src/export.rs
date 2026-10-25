//! Offline export (M3/M5): render a [`Project`]'s current pattern a number of times, or its
//! whole song chain once, into memory or a 16-bit stereo WAV file.
//!
//! This is UI-thread code: it builds its own [`Engine`] with [`new_engine`], so it never
//! touches the live audio stream, and it is free to allocate.
//!
//! Length formula (frames, per channel):
//!
//! ```text
//! samples_per_step = sample_rate * 60 / bpm / 4          (f64, see Transport::samples_per_step)
//! bar_frames       = round(16 * samples_per_step)
//! loop:  total     = repeats * bars(current pattern) * bar_frames + tail
//! song:  total     = sum over chain (repeats * bars(pattern)) * bar_frames + tail
//! tail             = round(TAIL_SECONDS * sample_rate)
//! ```
//!
//! A pattern of 16/32/64 steps is 1/2/4 bars, so one "repeat" is the pattern's full length.
//! An empty chain plays the current pattern once. At 120 BPM / 48 kHz one bar is exactly
//! 96 000 frames, so `render_loop(p, 1, 48_000.0)` of a one-bar pattern is
//! `(96_000 + 96_000) * 2` interleaved samples.

use std::fmt;
use std::path::Path;

use crate::engine::{Engine, new_engine};
use crate::pattern::PlayMode;
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

/// Bars rendered before the tail: the chain's bars when `song`, else `repeats` times the
/// current pattern's bars.
#[must_use]
pub fn export_bars(project: &Project, repeats: u32, song: bool) -> usize {
    if song {
        project.bank.song_bars()
    } else {
        repeats as usize * project.pattern().bars()
    }
}

/// Total frames [`render_loop`] (`song == false`) or [`render_song`] (`song == true`)
/// produce: `export_bars * bar_frames + tail_frames`. `repeats` is ignored for a song.
#[must_use]
pub fn export_frames(project: &Project, repeats: u32, song: bool, sample_rate: f32) -> usize {
    export_bars(project, repeats, song) * bar_frames(project.bpm, sample_rate)
        + tail_frames(sample_rate)
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

/// Renders the current pattern `repeats` times (one repeat = its full 1/2/4-bar length) plus
/// a [`TAIL_SECONDS`] tail into interleaved stereo, in pattern mode.
///
/// A fresh engine is created at `sample_rate`, the project applied, the transport started,
/// exactly `repeats * bars * bar_frames` frames rendered in [`BLOCK_FRAMES`] blocks (last
/// block partial), then the transport stopped and the tail rendered. The result has
/// `export_frames(project, repeats, false, sample_rate) * 2` samples.
///
/// `repeats` is clamped into `1..=MAX_REPEATS` and a non-finite or non-positive sample rate
/// falls back to 48 kHz; use [`render_loop_wav`] (or [`validate`]) for strict checking.
#[must_use]
pub fn render_loop(project: &Project, repeats: u32, sample_rate: f32) -> Vec<f32> {
    let repeats = repeats.clamp(1, MAX_REPEATS);
    render_project(project, repeats, false, sample_rate)
}

/// Renders the whole song chain once (every entry, honouring repeats) plus a
/// [`TAIL_SECONDS`] tail into interleaved stereo, in song mode. An empty chain renders the
/// current pattern once. The result has `export_frames(project, 1, true, sample_rate) * 2`
/// samples. A bad sample rate falls back to 48 kHz; see [`render_song_wav`] for strict
/// checking.
#[must_use]
pub fn render_song(project: &Project, sample_rate: f32) -> Vec<f32> {
    render_project(project, 1, true, sample_rate)
}

/// Shared body of [`render_loop`] and [`render_song`].
fn render_project(project: &Project, repeats: u32, song: bool, sample_rate: f32) -> Vec<f32> {
    let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        48_000.0
    };
    let (_handle, mut engine) = new_engine(sample_rate);
    project.apply_to(&mut engine);
    engine.set_play_mode(if song {
        PlayMode::Song
    } else {
        PlayMode::Pattern
    });

    // Use the engine's own (clamped, normalised) tempo and bank so the length matches what
    // it plays.
    let bars = if song {
        engine.bank().song_bars()
    } else {
        repeats as usize * engine.bank().current_pattern().bars()
    };
    let loop_frames = bars * bar_frames(engine.bpm(), sample_rate);
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
    write_wav(&samples, sample_rate, path)
}

/// Renders like [`render_song`] and writes a 16-bit PCM stereo WAV to `path`.
/// `sample_rate` must be finite and positive.
pub fn render_song_wav(
    project: &Project,
    sample_rate: f32,
    path: impl AsRef<Path>,
) -> Result<ExportStats, ExportError> {
    validate(1, sample_rate)?;
    let samples = render_song(project, sample_rate);
    write_wav(&samples, sample_rate, path)
}

/// Writes interleaved stereo samples as a 16-bit PCM WAV and summarises them.
fn write_wav(
    samples: &[f32],
    sample_rate: f32,
    path: impl AsRef<Path>,
) -> Result<ExportStats, ExportError> {
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
    for &s in samples {
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
    use crate::pattern::{ChainEntry, PatternLength};

    #[test]
    fn length_formula_at_120_bpm_48k() {
        assert_eq!(bar_frames(120.0, 48_000.0), 96_000);
        assert_eq!(tail_frames(48_000.0), 96_000);
        let p = Project::default();
        assert_eq!(export_frames(&p, 1, false, 48_000.0), 192_000);
        assert_eq!(export_frames(&p, 4, false, 48_000.0), 4 * 96_000 + 96_000);
        // The default chain is [A x1]: the song is one bar.
        assert_eq!(export_frames(&p, 8, true, 48_000.0), 192_000);
        // Out-of-range tempo is clamped like the engine clamps it (60..=200 BPM).
        assert_eq!(bar_frames(1.0, 48_000.0), bar_frames(60.0, 48_000.0));
        // Non-integer step lengths round the whole bar, not each step.
        // 44_100 * 60 / 130 / 4 = 5088.4615.. per step; 16 steps = 81415.38.. -> 81415.
        assert_eq!(bar_frames(130.0, 44_100.0), 81_415);
    }

    #[test]
    fn bars_follow_pattern_lengths_and_the_chain() {
        let mut p = Project::default();
        p.bank.set_length(0, PatternLength::Bar2);
        p.bank.set_length(1, PatternLength::Bar4);
        assert_eq!(export_bars(&p, 2, false), 4);
        p.bank.truncate(0);
        p.bank.push(ChainEntry::new(0, 2));
        p.bank.push(ChainEntry::new(1, 1));
        p.bank.push(ChainEntry::new(2, 3));
        assert_eq!(export_bars(&p, 1, true), 2 * 2 + 4 + 3);
        p.bank.truncate(0);
        p.bank.current = 1;
        assert_eq!(
            export_bars(&p, 1, true),
            4,
            "an empty chain plays the current pattern"
        );
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
