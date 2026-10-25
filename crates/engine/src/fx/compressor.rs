//! Feed-forward peak compressor (M11 insert slot).
//!
//! Classic topology: a peak detector with attack / release, a gain computer with a soft knee,
//! make-up gain and a dry/wet mix.
//!
//! - **Detector**: stereo-linked, `x = max(|l|, |r|)`, into a peak detector with an
//!   instant attack and an exponential release (`env = max(x, env * (1 - release_coeff))`,
//!   `coeff = 1 - exp(-1 / (sr * t))`), so the envelope of a steady sine sits at its true
//!   peak (within a few hundredths of a dB) instead of under it.
//! - **Gain computer** (in dB, on the envelope): below `threshold - 3 dB` nothing happens;
//!   above `threshold + 3 dB` the excess is divided by `ratio`; in between a quadratic
//!   [`KNEE_DB`]-wide knee joins the two lines without a kink. The result is the target
//!   *gain reduction* in dB.
//! - **Attack**: the applied reduction climbs towards the target with the attack time
//!   constant and follows it straight down (the detector's release already shapes the way
//!   down), so a transient is caught within the attack time and the release is exactly the
//!   detector's. The applied reduction is exposed as [`Compressor::gain_reduction_db`]
//!   (`>= 0`, `0` = none) for the mixer's GR meter.
//! - **Output**: `wet = x * db_to_gain(-reduction) * makeup`, then `x + (wet - x) * mix`.
//!   With the signal under the threshold, make-up at 0 dB and mix at 1 the output is the
//!   input bit for bit, so the slot is transparent on quiet material.
//!
//! Parameter changes apply at once (the envelope follower smooths their audible effect; a
//! threshold jump on a hot signal is a gain step, as in any dynamics processor). Real-time
//! safe: `Copy`, no allocation, no panics, non-finite input treated as silence, the envelope
//! denormal-flushed.

use crate::dsp::{db_to_gain, flush_denormal, gain_to_db};

/// Lowest threshold, dBFS.
pub const MIN_THRESHOLD_DB: f32 = -60.0;
/// Highest threshold, dBFS.
pub const MAX_THRESHOLD_DB: f32 = 0.0;
/// Lowest ratio (`1:1`, no compression).
pub const MIN_RATIO: f32 = 1.0;
/// Highest ratio (`20:1`, near limiting).
pub const MAX_RATIO: f32 = 20.0;
/// Shortest attack, milliseconds.
pub const MIN_ATTACK_MS: f32 = 0.1;
/// Longest attack, milliseconds.
pub const MAX_ATTACK_MS: f32 = 100.0;
/// Shortest release, milliseconds.
pub const MIN_RELEASE_MS: f32 = 10.0;
/// Longest release, milliseconds.
pub const MAX_RELEASE_MS: f32 = 1000.0;
/// Largest make-up gain, dB.
pub const MAX_MAKEUP_DB: f32 = 24.0;
/// Width of the soft knee, dB (centred on the threshold).
pub const KNEE_DB: f32 = 6.0;
/// Envelope levels below this are treated as this many dB (the log of zero).
const FLOOR_DB: f32 = -180.0;

/// Default threshold, dBFS.
pub const DEFAULT_THRESHOLD_DB: f32 = -18.0;
/// Default ratio.
pub const DEFAULT_RATIO: f32 = 4.0;
/// Default attack, milliseconds.
pub const DEFAULT_ATTACK_MS: f32 = 10.0;
/// Default release, milliseconds.
pub const DEFAULT_RELEASE_MS: f32 = 100.0;

/// One-pole coefficient for a time constant of `ms` at `sample_rate` (instant for `ms <= 0`).
fn time_coefficient(sample_rate: f32, ms: f32) -> f32 {
    if !ms.is_finite() || ms <= 0.0 || !sample_rate.is_finite() || sample_rate <= 0.0 {
        return 1.0;
    }
    (1.0 - (-1.0 / (sample_rate * ms * 0.001)).exp()).clamp(0.0, 1.0)
}

/// Clamps `v` into `min..=max`; non-finite values become `fallback`.
fn clamp_or(v: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(min, max)
    } else {
        fallback
    }
}

/// Feed-forward peak compressor with a soft knee. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Compressor {
    sample_rate: f32,
    threshold_db: f32,
    ratio: f32,
    attack_ms: f32,
    release_ms: f32,
    makeup_db: f32,
    mix: f32,
    attack_coeff: f32,
    release_coeff: f32,
    /// Linear make-up gain.
    makeup: f32,
    /// Envelope follower state (linear peak).
    env: f32,
    /// Gain reduction of the last processed frame, dB (`>= 0`).
    gr_db: f32,
}

impl Compressor {
    /// A compressor for `sample_rate` (non-finite or non-positive rates fall back to 48 kHz)
    /// at the defaults: threshold -18 dBFS, ratio 4:1, attack 10 ms, release 100 ms, no
    /// make-up, mix 1.
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let mut c = Self {
            sample_rate,
            threshold_db: DEFAULT_THRESHOLD_DB,
            ratio: DEFAULT_RATIO,
            attack_ms: DEFAULT_ATTACK_MS,
            release_ms: DEFAULT_RELEASE_MS,
            makeup_db: 0.0,
            mix: 1.0,
            attack_coeff: 1.0,
            release_coeff: 1.0,
            makeup: 1.0,
            env: 0.0,
            gr_db: 0.0,
        };
        c.set_attack_ms(DEFAULT_ATTACK_MS);
        c.set_release_ms(DEFAULT_RELEASE_MS);
        c
    }

    /// Sample rate the compressor was built for.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Sets the threshold in dBFS (clamped to `MIN_THRESHOLD_DB..=MAX_THRESHOLD_DB`).
    pub fn set_threshold_db(&mut self, db: f32) {
        self.threshold_db = clamp_or(db, MIN_THRESHOLD_DB, MAX_THRESHOLD_DB, self.threshold_db);
    }

    /// Sets the ratio (clamped to `MIN_RATIO..=MAX_RATIO`).
    pub fn set_ratio(&mut self, ratio: f32) {
        self.ratio = clamp_or(ratio, MIN_RATIO, MAX_RATIO, self.ratio);
    }

    /// Sets the attack time in milliseconds (clamped to `MIN_ATTACK_MS..=MAX_ATTACK_MS`).
    pub fn set_attack_ms(&mut self, ms: f32) {
        self.attack_ms = clamp_or(ms, MIN_ATTACK_MS, MAX_ATTACK_MS, self.attack_ms);
        self.attack_coeff = time_coefficient(self.sample_rate, self.attack_ms);
    }

    /// Sets the release time in milliseconds (clamped to `MIN_RELEASE_MS..=MAX_RELEASE_MS`).
    pub fn set_release_ms(&mut self, ms: f32) {
        self.release_ms = clamp_or(ms, MIN_RELEASE_MS, MAX_RELEASE_MS, self.release_ms);
        self.release_coeff = time_coefficient(self.sample_rate, self.release_ms);
    }

    /// Sets the make-up gain in dB (clamped to `0..=MAX_MAKEUP_DB`). Exactly unity at 0 dB.
    pub fn set_makeup_db(&mut self, db: f32) {
        self.makeup_db = clamp_or(db, 0.0, MAX_MAKEUP_DB, self.makeup_db);
        self.makeup = if self.makeup_db == 0.0 {
            1.0
        } else {
            db_to_gain(self.makeup_db)
        };
    }

    /// Sets the dry/wet mix in `0..=1`.
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = clamp_or(mix, 0.0, 1.0, self.mix);
    }

    /// Current threshold, dBFS.
    #[must_use]
    pub fn threshold_db(&self) -> f32 {
        self.threshold_db
    }

    /// Current ratio.
    #[must_use]
    pub fn ratio(&self) -> f32 {
        self.ratio
    }

    /// Current attack, milliseconds.
    #[must_use]
    pub fn attack_ms(&self) -> f32 {
        self.attack_ms
    }

    /// Current release, milliseconds.
    #[must_use]
    pub fn release_ms(&self) -> f32 {
        self.release_ms
    }

    /// Current make-up gain, dB.
    #[must_use]
    pub fn makeup_db(&self) -> f32 {
        self.makeup_db
    }

    /// Current dry/wet mix.
    #[must_use]
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Envelope follower level (linear peak) after the last frame.
    #[must_use]
    pub fn envelope(&self) -> f32 {
        self.env
    }

    /// Gain reduction applied to the last frame, in dB (`>= 0`; `0` = no compression). Read
    /// it after `process` for a GR meter.
    #[must_use]
    pub fn gain_reduction_db(&self) -> f32 {
        self.gr_db
    }

    /// The static gain-reduction curve: how many dB the compressor takes off a signal whose
    /// detector level is `level_db` (soft knee of [`KNEE_DB`] around the threshold).
    #[must_use]
    pub fn reduction_db(&self, level_db: f32) -> f32 {
        let over = level_db - self.threshold_db;
        let half = KNEE_DB * 0.5;
        // Output-minus-input slope above the knee: `1/ratio - 1` (negative).
        let slope = 1.0 / self.ratio - 1.0;
        let delta = if over <= -half {
            0.0
        } else if over >= half {
            slope * over
        } else {
            let t = over + half;
            slope * t * t / (2.0 * KNEE_DB)
        };
        (-delta).max(0.0)
    }

    /// Clears the envelope and the GR reading (project load; between sounds).
    pub fn reset(&mut self) {
        self.env = 0.0;
        self.gr_db = 0.0;
    }

    /// Processes one stereo frame. Non-finite inputs are treated as `0.0`; the output is
    /// always finite. Transparent (bit for bit) while the envelope stays under the knee with
    /// make-up 0 dB and mix 1.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let l = if l.is_finite() { l } else { 0.0 };
        let r = if r.is_finite() { r } else { 0.0 };
        let x = l.abs().max(r.abs());
        // Peak detector: instant attack, exponential release.
        self.env = if x > self.env {
            x
        } else {
            flush_denormal(self.env - self.release_coeff * self.env)
        };
        let level_db = if self.env > 1.0e-9 {
            gain_to_db(self.env)
        } else {
            FLOOR_DB
        };
        let target = self.reduction_db(level_db);
        // The applied reduction attacks towards the target and follows it straight down.
        self.gr_db = if target > self.gr_db {
            self.gr_db + self.attack_coeff * (target - self.gr_db)
        } else {
            target
        };
        let gain = if self.gr_db > 0.0 {
            db_to_gain(-self.gr_db) * self.makeup
        } else {
            self.makeup
        };
        let (wl, wr) = (l * gain, r * gain);
        (l + (wl - l) * self.mix, r + (wr - r) * self.mix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn sine(n: usize, hz: f32, db: f32) -> Vec<f32> {
        let amp = db_to_gain(db);
        (0..n)
            .map(|i| (i as f32 * core::f32::consts::TAU * hz / SR).sin() * amp)
            .collect()
    }

    fn peak(buf: &[f32]) -> f32 {
        buf.iter().fold(0.0_f32, |m, v| m.max(v.abs()))
    }

    #[test]
    fn defaults_and_clamping() {
        let mut c = Compressor::new(SR);
        assert_eq!(c.threshold_db(), -18.0);
        assert_eq!(c.ratio(), 4.0);
        assert_eq!(c.attack_ms(), 10.0);
        assert_eq!(c.release_ms(), 100.0);
        assert_eq!(c.makeup_db(), 0.0);
        assert_eq!(c.mix(), 1.0);
        assert_eq!(c.gain_reduction_db(), 0.0);
        c.set_threshold_db(5.0);
        assert_eq!(c.threshold_db(), 0.0);
        c.set_ratio(0.5);
        assert_eq!(c.ratio(), 1.0);
        c.set_ratio(f32::NAN);
        assert_eq!(c.ratio(), 1.0);
        c.set_attack_ms(1000.0);
        assert_eq!(c.attack_ms(), 100.0);
        c.set_release_ms(1.0);
        assert_eq!(c.release_ms(), 10.0);
        c.set_makeup_db(-3.0);
        assert_eq!(c.makeup_db(), 0.0);
        c.set_mix(7.0);
        assert_eq!(c.mix(), 1.0);
        assert_eq!(Compressor::new(0.0).sample_rate(), 48_000.0);
    }

    #[test]
    fn static_curve_has_a_soft_knee() {
        let c = Compressor::new(SR);
        assert_eq!(c.reduction_db(-40.0), 0.0);
        assert_eq!(c.reduction_db(-21.0), 0.0, "knee starts 3 dB under");
        // 15 dB over the threshold at 4:1: 15 * 3/4 taken off.
        assert!((c.reduction_db(-3.0) - 11.25).abs() < 1e-5);
        // Inside the knee the curve is continuous and between the two lines.
        let at_threshold = c.reduction_db(-18.0);
        assert!(at_threshold > 0.0 && at_threshold < 3.0 * 0.75);
        let just_out = c.reduction_db(-15.0);
        assert!((just_out - 3.0 * 0.75).abs() < 1e-5, "{just_out}");
        let mut prev = 0.0;
        for i in 0..100 {
            let level = -30.0 + i as f32 * 0.3;
            let g = c.reduction_db(level);
            assert!(g >= prev - 1e-6, "monotonic at {level}");
            prev = g;
        }
    }

    #[test]
    fn loud_sine_is_reduced_to_the_curve_and_quiet_one_is_untouched() {
        let mut c = Compressor::new(SR);
        let loud = sine(48_000, 1000.0, -3.0);
        let out: Vec<f32> = loud.iter().map(|&x| c.process(x, x).0).collect();
        // After a few attack times the output peak sits near -14.25 dBFS.
        let settled = gain_to_db(peak(&out[24_000..]));
        assert!(
            (settled + 14.25).abs() < 0.5,
            "settled peak {settled} dBFS, expected about -14.25"
        );
        assert!(
            (c.gain_reduction_db() - 11.25).abs() < 0.5,
            "{}",
            c.gain_reduction_db()
        );
        // The first samples pass almost untouched (the envelope has not caught up yet).
        assert!(peak(&out[..20]) > peak(&loud[..20]) * 0.9);

        let mut c = Compressor::new(SR);
        let quiet = sine(4_800, 1000.0, -30.0);
        for &x in &quiet {
            let (l, r) = c.process(x, -x);
            assert_eq!(l, x);
            assert_eq!(r, -x);
        }
        assert_eq!(c.gain_reduction_db(), 0.0);
    }

    #[test]
    fn makeup_mix_release_and_reset() {
        let mut c = Compressor::new(SR);
        c.set_makeup_db(6.0);
        let (l, _) = c.process(0.01, 0.01);
        assert!((l - 0.01 * db_to_gain(6.0)).abs() < 1e-7);
        c.set_makeup_db(0.0);
        c.set_mix(0.0);
        for i in 0..4_800 {
            let x = (i as f32 * 0.1).sin() * 0.9;
            assert_eq!(c.process(x, x).0, x, "mix 0 is dry");
        }
        assert!(c.gain_reduction_db() > 0.0, "the detector still runs");
        // Release: after the burst ends the envelope decays (about 87 dB per second at a
        // 100 ms release) and the GR reading falls to 0 well before that.
        c.set_mix(1.0);
        let mut fell_to_zero_at = None;
        for i in 0..48_000 {
            c.process(0.0, 0.0);
            if fell_to_zero_at.is_none() && c.gain_reduction_db() == 0.0 {
                fell_to_zero_at = Some(i);
            }
        }
        assert_eq!(c.gain_reduction_db(), 0.0);
        let at = fell_to_zero_at.expect("the reduction releases");
        // 11.25 dB of reduction at 8.7 dB per time constant: gone within ~2.5 constants.
        assert!((4_800..15_000).contains(&at), "released at sample {at}");
        assert!(c.envelope() < 1.0e-4, "{}", c.envelope());
        for &x in &sine(4_800, 1000.0, -1.0) {
            c.process(x, x);
        }
        assert!(c.envelope() > 0.5);
        c.reset();
        assert_eq!(c.envelope(), 0.0);
        assert_eq!(c.gain_reduction_db(), 0.0);
        let (l, r) = c.process(f32::NAN, f32::INFINITY);
        assert_eq!((l, r), (0.0, 0.0));
    }
}
