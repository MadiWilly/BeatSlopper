//! Procedurally synthesized drum kit (M3): kick, snare, clap, closed hat, open hat.
//!
//! No sample files and no `rand`: every voice is a small `Copy` state machine built from the
//! [`envelope::Decay`] envelope, the [`filter::Svf3`] multi-mode filter and the
//! [`noise::Noise`] xorshift PRNG. [`DrumKit::tick`] never allocates, locks or panics.
//!
//! - [`kick::Kick`]: sine, exponential pitch sweep 150 -> 50 Hz (tau 50 ms), soft clip, 2 ms
//!   noise click, 350 ms decay.
//! - [`snare::Snare`]: 180 + 330 Hz tones (120 ms) plus 1.5-4 kHz band-passed noise (180 ms).
//! - [`clap::Clap`]: four 1-2 kHz band-passed noise bursts 10 ms apart, then a 150 ms tail.
//! - [`hats::Hat`]: six inharmonic squares plus noise, high-passed at 6 kHz with a resonant
//!   peak; closed 60 ms, open 400 ms. The closed hat chokes the open hat with a 5 ms fade.
//!
//! Tune (semitones, `-12..=12`) scales tonal parts and filter centres by `2^(st / 12)` at
//! trigger time; volume (dB, `-24..=6`) is a smoothed per-row gain; velocity scales the
//! amplitude linearly. Each drum peaks at about -7 dBFS at velocity 1 and 0 dB, so a kick
//! and a hat landing together stay under full scale ahead of the master limiter.

pub mod clap;
pub mod envelope;
pub mod filter;
pub mod hats;
pub mod kick;
pub mod noise;
pub mod snare;
pub mod voice;

use crate::params::Smoother;
use clap::Clap;
use hats::Hat;
use kick::Kick;
use snare::Snare;
use voice::Retrig;

/// The five drum rows of the step sequencer, in display order (top to bottom).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrumRow {
    /// Sine pitch sweep plus a click transient.
    Kick,
    /// Tone plus filtered noise.
    Snare,
    /// Several short noise bursts.
    Clap,
    /// Short high-passed noise / metallic burst.
    ClosedHat,
    /// Longer high-passed noise / metallic burst.
    OpenHat,
}

impl DrumRow {
    /// All rows in display order.
    pub const ALL: [DrumRow; 5] = [
        DrumRow::Kick,
        DrumRow::Snare,
        DrumRow::Clap,
        DrumRow::ClosedHat,
        DrumRow::OpenHat,
    ];

    /// Number of drum rows.
    pub const COUNT: usize = Self::ALL.len();

    /// Index of this row in [`DrumRow::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Short label used in the sequencer grid.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            DrumRow::Kick => "Kick",
            DrumRow::Snare => "Snare",
            DrumRow::Clap => "Clap",
            DrumRow::ClosedHat => "CH",
            DrumRow::OpenHat => "OH",
        }
    }
}

/// Lowest per-row volume, dB.
pub const MIN_VOLUME_DB: f32 = -24.0;
/// Highest per-row volume, dB.
pub const MAX_VOLUME_DB: f32 = 6.0;
/// Lowest per-row tune, semitones.
pub const MIN_TUNE_SEMITONES: f32 = -12.0;
/// Highest per-row tune, semitones.
pub const MAX_TUNE_SEMITONES: f32 = 12.0;
/// Smoothing time of the per-row volume, ms.
const VOLUME_SMOOTH_MS: f32 = 10.0;

/// The procedural drum kit: five voices, one mono output.
///
/// Everything is pre-allocated in [`DrumKit::new`]; the whole struct is plain data that can be
/// cloned or compared (the engine's offline renderer relies on that).
#[derive(Debug, Clone, PartialEq)]
pub struct DrumKit {
    sample_rate: f32,
    volume: [Smoother; DrumRow::COUNT],
    tune: [f32; DrumRow::COUNT],
    kick: Retrig<Kick>,
    snare: Retrig<Snare>,
    clap: Retrig<Clap>,
    closed_hat: Retrig<Hat>,
    open_hat: Retrig<Hat>,
}

impl DrumKit {
    /// Creates a kit for `sample_rate`; everything is pre-allocated here.
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        DrumKit {
            sample_rate,
            volume: [Smoother::new(sample_rate, VOLUME_SMOOTH_MS, 1.0); DrumRow::COUNT],
            tune: [0.0; DrumRow::COUNT],
            kick: Retrig::new(sample_rate, Kick::new(sample_rate)),
            snare: Retrig::new(sample_rate, Snare::new(sample_rate)),
            clap: Retrig::new(sample_rate, Clap::new(sample_rate)),
            closed_hat: Retrig::new(sample_rate, Hat::closed(sample_rate)),
            open_hat: Retrig::new(sample_rate, Hat::open(sample_rate)),
        }
    }

    /// Sample rate the kit runs at.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Fires `row` at `velocity` (`0.0..=1.0`, clamped; non-finite is ignored). Retriggering a
    /// sounding voice restarts it without a click; the closed hat chokes the open hat.
    pub fn trigger(&mut self, row: DrumRow, velocity: f32) {
        if !velocity.is_finite() {
            return;
        }
        let velocity = velocity.clamp(0.0, 1.0);
        let tune = semitones_to_ratio(self.tune[row.index()]);
        match row {
            DrumRow::Kick => self.kick.trigger(velocity, tune),
            DrumRow::Snare => self.snare.trigger(velocity, tune),
            DrumRow::Clap => self.clap.trigger(velocity, tune),
            DrumRow::ClosedHat => {
                self.open_hat.choke();
                self.closed_hat.trigger(velocity, tune);
            }
            DrumRow::OpenHat => self.open_hat.trigger(velocity, tune),
        }
    }

    /// Per-row volume in dB (clamped to `-24..=6`; non-finite is ignored). Smoothed over 10 ms.
    pub fn set_volume(&mut self, row: DrumRow, db: f32) {
        if db.is_finite() {
            let gain = crate::dsp::db_to_gain(db.clamp(MIN_VOLUME_DB, MAX_VOLUME_DB));
            self.volume[row.index()].set_target(gain);
        }
    }

    /// Per-row tune in semitones (clamped to `-12..=12`; non-finite is ignored). Applies to
    /// the next trigger of the row.
    pub fn set_tune(&mut self, row: DrumRow, semitones: f32) {
        if semitones.is_finite() {
            self.tune[row.index()] = semitones.clamp(MIN_TUNE_SEMITONES, MAX_TUNE_SEMITONES);
        }
    }

    /// Current per-row volume gain target (linear).
    #[must_use]
    pub fn volume_gain(&self, row: DrumRow) -> f32 {
        self.volume[row.index()].target()
    }

    /// Current per-row tune in semitones.
    #[must_use]
    pub fn tune(&self, row: DrumRow) -> f32 {
        self.tune[row.index()]
    }

    /// Next mono sample of the whole kit.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let [vk, vs, vc, vch, voh] = &mut self.volume;
        self.kick.tick() * vk.tick()
            + self.snare.tick() * vs.tick()
            + self.clap.tick() * vc.tick()
            + self.closed_hat.tick() * vch.tick()
            + self.open_hat.tick() * voh.tick()
    }

    /// `true` while any drum voice is still sounding.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.kick.is_active()
            || self.snare.is_active()
            || self.clap.is_active()
            || self.closed_hat.is_active()
            || self.open_hat.is_active()
    }

    /// Silences every voice immediately (volumes and tunes are kept).
    pub fn reset(&mut self) {
        self.kick.reset();
        self.snare.reset();
        self.clap.reset();
        self.closed_hat.reset();
        self.open_hat.reset();
    }
}

/// Frequency ratio for a shift of `semitones`.
#[inline]
#[must_use]
pub fn semitones_to_ratio(semitones: f32) -> f32 {
    2.0_f32.powf(semitones / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semitone_ratio() {
        assert_eq!(semitones_to_ratio(0.0), 1.0);
        assert!((semitones_to_ratio(12.0) - 2.0).abs() < 1.0e-6);
        assert!((semitones_to_ratio(-12.0) - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn clamps_volume_and_tune_and_ignores_non_finite() {
        let mut kit = DrumKit::new(48_000.0);
        kit.set_volume(DrumRow::Kick, 100.0);
        assert!((kit.volume_gain(DrumRow::Kick) - crate::dsp::db_to_gain(6.0)).abs() < 1.0e-6);
        kit.set_volume(DrumRow::Kick, f32::NAN);
        assert!((kit.volume_gain(DrumRow::Kick) - crate::dsp::db_to_gain(6.0)).abs() < 1.0e-6);
        kit.set_tune(DrumRow::Snare, -100.0);
        assert_eq!(kit.tune(DrumRow::Snare), -12.0);
        kit.set_tune(DrumRow::Snare, f32::INFINITY);
        assert_eq!(kit.tune(DrumRow::Snare), -12.0);
    }

    #[test]
    fn idle_kit_is_silent_and_inactive() {
        let mut kit = DrumKit::new(48_000.0);
        assert!(!kit.is_active());
        for _ in 0..1_000 {
            assert_eq!(kit.tick(), 0.0);
        }
        kit.trigger(DrumRow::Kick, f32::NAN);
        assert!(!kit.is_active());
    }
}
