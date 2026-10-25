//! Parameter handling: ids, ranges and defaults for every user-facing parameter, the
//! [`Smoother`] that keeps UI changes free of zipper noise, and the [`Adsr`] amplitude envelope.
//!
//! Every audible parameter (gains, cutoff, mix amounts, ...) should go through a [`Smoother`]
//! rather than being applied as a step. Envelope *times* are the exception: they only change
//! how a running envelope proceeds from here on, which is click-free by construction.

use core::ops::RangeInclusive;

use crate::dsp::flush_denormal;

/// Default smoothing time used for gain-like parameters (about 10 ms).
pub const DEFAULT_SMOOTH_MS: f32 = 10.0;

/// Relative distance to the target below which the smoother snaps and reports settled.
///
/// Relative to `max(|target|, 1.0)`, so a gain settles within -80 dB of its target. Needed
/// because an `f32` one-pole stalls when `coeff * diff` drops below half an ulp of `current`.
const SNAP_EPSILON: f32 = 1.0e-4;

/// Identifier of a user-facing engine parameter, sent with `Message::SetParam`.
///
/// M1 ships the amplitude envelope and the master gain; M2 adds oscillator, filter, LFO and FX
/// ids. The enum is deliberately exhaustive (no `#[non_exhaustive]`) so the UI can `match` on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParamId {
    /// Oscillator 1 waveform index: 0 sine, 1 saw, 2 square, 3 triangle.
    Osc1Wave,
    /// Oscillator 1 detune, cents.
    Osc1Detune,
    /// Oscillator 1 level, `0.0..=1.0`.
    Osc1Mix,
    /// Oscillator 2 waveform index: 0 sine, 1 saw, 2 square, 3 triangle.
    Osc2Wave,
    /// Oscillator 2 detune, cents.
    Osc2Detune,
    /// Oscillator 2 level, `0.0..=1.0`.
    Osc2Mix,
    /// Lowpass filter cutoff, Hz (log scale in the UI).
    FilterCutoff,
    /// Lowpass filter resonance, `0.0..=1.0`.
    FilterResonance,
    /// Filter envelope amount, `-1.0..=1.0` (+-4 octaves driven by the amp envelope).
    FilterEnvAmount,
    /// Amplitude envelope attack time, seconds.
    AmpAttack,
    /// Amplitude envelope decay time, seconds.
    AmpDecay,
    /// Amplitude envelope sustain level, `0.0..=1.0`.
    AmpSustain,
    /// Amplitude envelope release time, seconds.
    AmpRelease,
    /// LFO rate, Hz.
    LfoRate,
    /// LFO depth, `0.0..=1.0`.
    LfoDepth,
    /// LFO target index: 0 pitch, 1 cutoff.
    LfoTarget,
    /// Glide (portamento) time, seconds; 0 = off.
    Glide,
    /// Drive amount, `0.0..=1.0`.
    DriveAmount,
    /// Drive dry/wet mix, `0.0..=1.0`.
    DriveMix,
    /// Delay time division index: 0 = 1/16, 1 = 1/8, 2 = 1/4, 3 = 1/2 (synced to BPM).
    DelayTime,
    /// Delay feedback, `0.0..=0.95`.
    DelayFeedback,
    /// Delay dry/wet mix, `0.0..=1.0`.
    DelayMix,
    /// Reverb room size, `0.0..=1.0`.
    ReverbSize,
    /// Reverb damping, `0.0..=1.0`.
    ReverbDamping,
    /// Reverb dry/wet mix, `0.0..=1.0`.
    ReverbMix,
    /// Master output gain, linear (`1.0` = unity, `4.0` = +12 dB).
    MasterGain,
    /// Soft limiter enabled: 0 off, 1 on.
    LimiterOn,
}

impl ParamId {
    /// Every parameter, in display order.
    pub const ALL: &'static [ParamId] = &[
        ParamId::Osc1Wave,
        ParamId::Osc1Detune,
        ParamId::Osc1Mix,
        ParamId::Osc2Wave,
        ParamId::Osc2Detune,
        ParamId::Osc2Mix,
        ParamId::FilterCutoff,
        ParamId::FilterResonance,
        ParamId::FilterEnvAmount,
        ParamId::AmpAttack,
        ParamId::AmpDecay,
        ParamId::AmpSustain,
        ParamId::AmpRelease,
        ParamId::LfoRate,
        ParamId::LfoDepth,
        ParamId::LfoTarget,
        ParamId::Glide,
        ParamId::DriveAmount,
        ParamId::DriveMix,
        ParamId::DelayTime,
        ParamId::DelayFeedback,
        ParamId::DelayMix,
        ParamId::ReverbSize,
        ParamId::ReverbDamping,
        ParamId::ReverbMix,
        ParamId::MasterGain,
        ParamId::LimiterOn,
    ];

    /// Number of parameters.
    pub const COUNT: usize = Self::ALL.len();

    /// Index of this parameter in [`ParamId::ALL`]; usable as an array index.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// `true` for the effect-chain parameters (drive, delay, reverb).
    #[must_use]
    pub const fn is_fx(self) -> bool {
        matches!(
            self,
            ParamId::DriveAmount
                | ParamId::DriveMix
                | ParamId::DelayTime
                | ParamId::DelayFeedback
                | ParamId::DelayMix
                | ParamId::ReverbSize
                | ParamId::ReverbDamping
                | ParamId::ReverbMix
        )
    }

    /// `true` for the synth-voice parameters (oscillators, filter, amp envelope, LFO, glide):
    /// everything the [`VoicePool`](crate::voice::VoicePool) consumes.
    #[must_use]
    pub const fn is_synth(self) -> bool {
        matches!(
            self,
            ParamId::Osc1Wave
                | ParamId::Osc1Detune
                | ParamId::Osc1Mix
                | ParamId::Osc2Wave
                | ParamId::Osc2Detune
                | ParamId::Osc2Mix
                | ParamId::FilterCutoff
                | ParamId::FilterResonance
                | ParamId::FilterEnvAmount
                | ParamId::AmpAttack
                | ParamId::AmpDecay
                | ParamId::AmpSustain
                | ParamId::AmpRelease
                | ParamId::LfoRate
                | ParamId::LfoDepth
                | ParamId::LfoTarget
                | ParamId::Glide
        )
    }

    /// `true` for parameters that are an index into a fixed list (waveform, LFO target, delay
    /// division, limiter switch) and should be shown as a segmented control.
    #[must_use]
    pub const fn is_discrete(self) -> bool {
        matches!(
            self,
            ParamId::Osc1Wave
                | ParamId::Osc2Wave
                | ParamId::LfoTarget
                | ParamId::DelayTime
                | ParamId::LimiterOn
        )
    }

    /// Value of a fresh project.
    #[must_use]
    pub const fn default_value(self) -> f32 {
        match self {
            ParamId::Osc1Wave => 1.0,
            ParamId::Osc1Detune => 0.0,
            ParamId::Osc1Mix => 0.7,
            ParamId::Osc2Wave => 0.0,
            ParamId::Osc2Detune => 0.0,
            ParamId::Osc2Mix => 0.45,
            ParamId::FilterCutoff => 2400.0,
            ParamId::FilterResonance => 0.22,
            ParamId::FilterEnvAmount => 0.4,
            ParamId::AmpAttack => 0.01,
            ParamId::AmpDecay => 0.2,
            ParamId::AmpSustain => 0.7,
            ParamId::AmpRelease => 0.3,
            ParamId::LfoRate => 4.5,
            ParamId::LfoDepth => 0.25,
            ParamId::LfoTarget => 1.0,
            ParamId::Glide => 0.0,
            ParamId::DriveAmount => 0.35,
            ParamId::DriveMix => 0.5,
            ParamId::DelayTime => 1.0,
            ParamId::DelayFeedback => 0.35,
            ParamId::DelayMix => 0.25,
            ParamId::ReverbSize => 0.6,
            ParamId::ReverbDamping => 0.4,
            ParamId::ReverbMix => 0.2,
            ParamId::MasterGain => 1.0,
            ParamId::LimiterOn => 1.0,
        }
    }

    /// Allowed range; values outside it are clamped by the engine.
    #[must_use]
    pub const fn range(self) -> RangeInclusive<f32> {
        match self {
            ParamId::Osc1Wave | ParamId::Osc2Wave | ParamId::DelayTime => 0.0..=3.0,
            ParamId::Osc1Detune | ParamId::Osc2Detune => -50.0..=50.0,
            ParamId::FilterCutoff => 20.0..=20000.0,
            ParamId::FilterEnvAmount => -1.0..=1.0,
            ParamId::AmpAttack | ParamId::AmpDecay => 0.001..=2.0,
            ParamId::AmpRelease => 0.001..=4.0,
            ParamId::LfoRate => 0.1..=20.0,
            ParamId::LfoTarget | ParamId::LimiterOn => 0.0..=1.0,
            ParamId::Glide => 0.0..=1.0,
            ParamId::DelayFeedback => 0.0..=0.95,
            ParamId::MasterGain => 0.0..=4.0,
            ParamId::Osc1Mix
            | ParamId::Osc2Mix
            | ParamId::FilterResonance
            | ParamId::AmpSustain
            | ParamId::LfoDepth
            | ParamId::DriveAmount
            | ParamId::DriveMix
            | ParamId::DelayMix
            | ParamId::ReverbSize
            | ParamId::ReverbDamping
            | ParamId::ReverbMix => 0.0..=1.0,
        }
    }

    /// Short label for knobs and sliders.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            ParamId::Osc1Wave | ParamId::Osc2Wave => "Wave",
            ParamId::Osc1Detune | ParamId::Osc2Detune => "Detune",
            ParamId::Osc1Mix | ParamId::Osc2Mix | ParamId::DriveMix => "Mix",
            ParamId::DelayMix | ParamId::ReverbMix => "Mix",
            ParamId::FilterCutoff => "Cutoff",
            ParamId::FilterResonance => "Res",
            ParamId::FilterEnvAmount => "Env",
            ParamId::AmpAttack => "A",
            ParamId::AmpDecay => "D",
            ParamId::AmpSustain => "S",
            ParamId::AmpRelease => "R",
            ParamId::LfoRate => "Rate",
            ParamId::LfoDepth => "Depth",
            ParamId::LfoTarget => "Target",
            ParamId::Glide => "Glide",
            ParamId::DriveAmount => "Amount",
            ParamId::DelayTime => "Time",
            ParamId::DelayFeedback => "Feedback",
            ParamId::ReverbSize => "Size",
            ParamId::ReverbDamping => "Damping",
            ParamId::MasterGain => "Master",
            ParamId::LimiterOn => "Limit",
        }
    }

    /// Clamps `value` into [`range`](Self::range). Non-finite values become the default.
    #[must_use]
    pub fn clamp(self, value: f32) -> f32 {
        if !value.is_finite() {
            return self.default_value();
        }
        let range = self.range();
        value.clamp(*range.start(), *range.end())
    }
}

/// One-pole (exponential) parameter smoother.
///
/// `current += coeff * (target - current)` per sample, where `coeff` is derived from a time
/// constant: after `time_ms` the value has covered about 63 % of a step, after `5 * time_ms`
/// more than 99 %. Real-time safe: `Copy`, no allocation, no panics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Smoother {
    current: f32,
    target: f32,
    coeff: f32,
}

impl Smoother {
    /// Creates a smoother resting at `initial` with the given time constant in milliseconds.
    ///
    /// A non-positive `time_ms` or `sample_rate` yields an instant (unsmoothed) parameter.
    #[must_use]
    pub fn new(sample_rate: f32, time_ms: f32, initial: f32) -> Self {
        Self {
            current: initial,
            target: initial,
            coeff: Self::coefficient(sample_rate, time_ms),
        }
    }

    /// Computes the one-pole coefficient for a time constant of `time_ms` at `sample_rate`.
    #[must_use]
    pub fn coefficient(sample_rate: f32, time_ms: f32) -> f32 {
        if !time_ms.is_finite() || time_ms <= 0.0 || !sample_rate.is_finite() || sample_rate <= 0.0
        {
            return 1.0;
        }
        let tau_samples = sample_rate * time_ms * 0.001;
        (1.0 - (-1.0 / tau_samples).exp()).clamp(0.0, 1.0)
    }

    /// Changes the time constant without disturbing the current value.
    pub fn set_time(&mut self, sample_rate: f32, time_ms: f32) {
        self.coeff = Self::coefficient(sample_rate, time_ms);
    }

    /// Sets the value the smoother glides towards. Non-finite targets are ignored.
    pub fn set_target(&mut self, target: f32) {
        if target.is_finite() {
            self.target = target;
        }
    }

    /// Jumps straight to `value` (both current and target), e.g. on project load.
    pub fn snap(&mut self, value: f32) {
        if value.is_finite() {
            self.current = value;
            self.target = value;
        }
    }

    /// The value the smoother is moving towards.
    #[must_use]
    pub fn target(&self) -> f32 {
        self.target
    }

    /// The most recent smoothed value (without advancing).
    #[must_use]
    pub fn current(&self) -> f32 {
        self.current
    }

    /// `true` when the current value has reached the target.
    #[must_use]
    pub fn is_settled(&self) -> bool {
        self.current == self.target
    }

    /// Advances one sample and returns the new smoothed value.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let diff = self.target - self.current;
        let eps = SNAP_EPSILON * self.target.abs().max(1.0);
        if diff.abs() <= eps {
            self.current = self.target;
        } else {
            self.current = flush_denormal(self.current + self.coeff * diff);
        }
        self.current
    }
}

/// Shortest allowed envelope stage, in seconds (1 ms): short enough to feel instant, long
/// enough that no stage ever produces a click.
pub const MIN_STAGE_SECONDS: f32 = 0.001;

/// Level below which a releasing envelope snaps to exactly `0.0` and reports idle (-80 dBFS).
///
/// Decay and release are one-pole curves that never quite reach their target, so the stage
/// *time* is defined as the time for the distance to the target to shrink by this factor.
pub const ENVELOPE_FLOOR: f32 = 1.0e-4;

/// Stage of an [`Adsr`] envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AdsrStage {
    /// Not sounding; level is exactly `0.0`.
    #[default]
    Idle,
    /// Rising linearly towards `1.0`.
    Attack,
    /// Falling exponentially towards the sustain level.
    Decay,
    /// Holding at the sustain level while the key is down.
    Sustain,
    /// Falling exponentially towards `0.0` after the key was released.
    Release,
}

/// Amplitude envelope: linear attack, exponential (one-pole) decay and release.
///
/// Times are set with [`set_times`](Self::set_times) and are clamped to at least
/// [`MIN_STAGE_SECONDS`]. The decay/release time is the time for the level to travel 80 dB
/// towards its target (see [`ENVELOPE_FLOOR`]), so a release of 50 ms is fully silent after
/// 50 ms. Retriggering ([`note_on`](Self::note_on)) restarts the attack **from the current
/// level**, never from zero, so repeated or stolen notes do not click.
///
/// Real-time safe: `Copy`, no allocation, no panics; the level is denormal-flushed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Adsr {
    stage: AdsrStage,
    level: f32,
    /// Per-sample increment during attack (`1 / attack_samples`).
    attack_inc: f32,
    /// Per-sample multiplier applied to the distance from the sustain level during decay.
    decay_ratio: f32,
    /// Per-sample multiplier applied to the level during release.
    release_ratio: f32,
    sustain: f32,
}

impl Default for Adsr {
    fn default() -> Self {
        Self::new(
            48_000.0,
            ParamId::AmpAttack.default_value(),
            ParamId::AmpDecay.default_value(),
            ParamId::AmpSustain.default_value(),
            ParamId::AmpRelease.default_value(),
        )
    }
}

impl Adsr {
    /// Creates an idle envelope with the given times (seconds) and sustain level (`0..=1`).
    #[must_use]
    pub fn new(sample_rate: f32, attack: f32, decay: f32, sustain: f32, release: f32) -> Self {
        let mut env = Self {
            stage: AdsrStage::Idle,
            level: 0.0,
            attack_inc: 1.0,
            decay_ratio: 0.0,
            release_ratio: 0.0,
            sustain: 1.0,
        };
        env.set_times(sample_rate, attack, decay, sustain, release);
        env
    }

    /// Number of samples a stage of `seconds` lasts, at least one sample and at least
    /// [`MIN_STAGE_SECONDS`]. Non-finite or non-positive inputs fall back to the minimum.
    fn stage_samples(sample_rate: f32, seconds: f32) -> f32 {
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let secs = if seconds.is_finite() {
            seconds.max(MIN_STAGE_SECONDS)
        } else {
            MIN_STAGE_SECONDS
        };
        (sr * secs).max(1.0)
    }

    /// Per-sample ratio so that a distance shrinks by [`ENVELOPE_FLOOR`] over `seconds`.
    fn ratio(sample_rate: f32, seconds: f32) -> f32 {
        (ENVELOPE_FLOOR.ln() / Self::stage_samples(sample_rate, seconds)).exp()
    }

    /// Re-times the envelope: `attack`, `decay`, `release` in seconds (each clamped to at least
    /// [`MIN_STAGE_SECONDS`]), `sustain` in `0.0..=1.0`. Takes effect immediately on a running
    /// envelope without disturbing its current level.
    pub fn set_times(
        &mut self,
        sample_rate: f32,
        attack: f32,
        decay: f32,
        sustain: f32,
        release: f32,
    ) {
        self.attack_inc = 1.0 / Self::stage_samples(sample_rate, attack);
        self.decay_ratio = Self::ratio(sample_rate, decay);
        self.release_ratio = Self::ratio(sample_rate, release);
        self.sustain = if sustain.is_finite() {
            sustain.clamp(0.0, 1.0)
        } else {
            ParamId::AmpSustain.default_value()
        };
    }

    /// Starts (or restarts) the attack from the current level.
    pub fn note_on(&mut self) {
        self.stage = AdsrStage::Attack;
    }

    /// Enters the release stage. No effect when idle.
    pub fn note_off(&mut self) {
        if self.stage != AdsrStage::Idle {
            self.stage = AdsrStage::Release;
        }
    }

    /// Silences the envelope instantly (level `0.0`, idle). Only for resets, never while
    /// audible: it is the one operation here that can click.
    pub fn reset(&mut self) {
        self.stage = AdsrStage::Idle;
        self.level = 0.0;
    }

    /// Current stage.
    #[must_use]
    pub fn stage(&self) -> AdsrStage {
        self.stage
    }

    /// Current level in `0.0..=1.0` (without advancing).
    #[must_use]
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Sustain level in `0.0..=1.0`.
    #[must_use]
    pub fn sustain(&self) -> f32 {
        self.sustain
    }

    /// `true` when the envelope is not sounding.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.stage == AdsrStage::Idle
    }

    /// `true` while the envelope is in its release stage.
    #[must_use]
    pub fn is_releasing(&self) -> bool {
        self.stage == AdsrStage::Release
    }

    /// Advances one sample and returns the new level in `0.0..=1.0`.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        match self.stage {
            AdsrStage::Idle => {
                self.level = 0.0;
            }
            AdsrStage::Attack => {
                self.level += self.attack_inc;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = AdsrStage::Decay;
                }
            }
            AdsrStage::Decay | AdsrStage::Sustain => {
                // Sustain also glides, so a live change of the sustain level is click-free.
                let dist = (self.level - self.sustain) * self.decay_ratio;
                if dist.abs() <= ENVELOPE_FLOOR {
                    self.level = self.sustain;
                    self.stage = AdsrStage::Sustain;
                } else {
                    self.level = self.sustain + dist;
                }
            }
            AdsrStage::Release => {
                self.level *= self.release_ratio;
                if self.level <= ENVELOPE_FLOOR {
                    self.level = 0.0;
                    self.stage = AdsrStage::Idle;
                }
            }
        }
        self.level = flush_denormal(self.level);
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn samples(seconds: f32) -> usize {
        (SR * seconds).round() as usize
    }

    #[test]
    fn reaches_about_63_percent_after_one_time_constant() {
        let sr = 48_000.0;
        let mut s = Smoother::new(sr, 10.0, 0.0);
        s.set_target(1.0);
        let n = (sr * 0.010) as usize;
        let mut v = 0.0;
        for _ in 0..n {
            v = s.tick();
        }
        assert!((v - 0.632).abs() < 0.01, "got {v}");
    }

    #[test]
    fn settles_exactly_on_target() {
        let mut s = Smoother::new(48_000.0, 10.0, 1.0);
        s.set_target(0.0);
        for _ in 0..48_000 {
            s.tick();
        }
        assert!(s.is_settled());
        assert_eq!(s.current(), 0.0);
    }

    #[test]
    fn zero_time_is_instant() {
        let mut s = Smoother::new(48_000.0, 0.0, 0.0);
        s.set_target(3.0);
        assert_eq!(s.tick(), 3.0);
    }

    #[test]
    fn ignores_non_finite_targets() {
        let mut s = Smoother::new(48_000.0, 10.0, 0.5);
        s.set_target(f32::NAN);
        assert_eq!(s.target(), 0.5);
        s.snap(f32::INFINITY);
        assert_eq!(s.current(), 0.5);
    }

    #[test]
    fn param_ids_are_consistent() {
        for (i, id) in ParamId::ALL.iter().enumerate() {
            assert_eq!(id.index(), i);
            let r = id.range();
            assert!(r.contains(&id.default_value()), "{id:?}");
            assert!(!id.label().is_empty());
            assert_eq!(id.clamp(f32::NAN), id.default_value());
            assert_eq!(id.clamp(1.0e9), *r.end());
            assert_eq!(id.clamp(-1.0e9), *r.start());
        }
        assert_eq!(ParamId::COUNT, 27);
        assert!(ParamId::DelayMix.is_fx() && !ParamId::FilterCutoff.is_fx());
        assert!(ParamId::FilterCutoff.is_synth() && !ParamId::DelayMix.is_synth());
        assert!(!ParamId::MasterGain.is_synth() && !ParamId::LimiterOn.is_synth());
        for id in ParamId::ALL {
            assert!(!(id.is_synth() && id.is_fx()), "{id:?}");
        }
        assert!(ParamId::Osc1Wave.is_discrete() && !ParamId::Osc1Mix.is_discrete());
        assert_eq!(ParamId::AmpAttack.default_value(), 0.01);
        assert_eq!(ParamId::AmpDecay.default_value(), 0.2);
        assert_eq!(ParamId::AmpSustain.default_value(), 0.7);
        assert_eq!(ParamId::AmpRelease.default_value(), 0.3);
        assert_eq!(ParamId::MasterGain.default_value(), 1.0);
    }

    #[test]
    fn adsr_attack_reaches_one_in_attack_time() {
        let mut env = Adsr::new(SR, 0.010, 0.1, 0.5, 0.1);
        assert!(env.is_idle());
        env.note_on();
        let n = samples(0.010);
        let mut peak = 0.0_f32;
        for _ in 0..n - 2 {
            let v = env.tick();
            assert!(v > peak, "attack must rise monotonically");
            peak = v;
            assert_eq!(env.stage(), AdsrStage::Attack);
        }
        // A couple of samples later we are at exactly 1.0 and into the decay.
        env.tick();
        env.tick();
        env.tick();
        assert!(env.level() <= 1.0);
        assert!(env.level() > 0.99, "level {}", env.level());
        assert_eq!(env.stage(), AdsrStage::Decay);
    }

    #[test]
    fn adsr_sustain_holds() {
        let mut env = Adsr::new(SR, 0.001, 0.050, 0.6, 0.1);
        env.note_on();
        for _ in 0..samples(0.2) {
            env.tick();
        }
        assert_eq!(env.stage(), AdsrStage::Sustain);
        for _ in 0..samples(1.0) {
            let v = env.tick();
            assert!((v - 0.6).abs() < 1e-5, "sustain drifted to {v}");
        }
        assert!(!env.is_idle());
        assert!(!env.is_releasing());
    }

    #[test]
    fn adsr_decay_approaches_sustain_within_decay_time() {
        let mut env = Adsr::new(SR, 0.001, 0.100, 0.3, 0.1);
        env.note_on();
        for _ in 0..samples(0.001) + 1 {
            env.tick();
        }
        assert_eq!(env.stage(), AdsrStage::Decay);
        let mut prev = env.level();
        for _ in 0..samples(0.100) {
            let v = env.tick();
            assert!(v <= prev + 1e-7, "decay must fall monotonically");
            prev = v;
        }
        assert!((env.level() - 0.3).abs() < 1e-3, "level {}", env.level());
    }

    #[test]
    fn adsr_release_decays_to_idle_within_release_time() {
        let mut env = Adsr::new(SR, 0.001, 0.010, 1.0, 0.050);
        env.note_on();
        for _ in 0..samples(0.05) {
            env.tick();
        }
        env.note_off();
        assert!(env.is_releasing());
        let mut prev = env.level();
        for _ in 0..samples(0.050) {
            let v = env.tick();
            assert!(v <= prev, "release must fall monotonically");
            prev = v;
        }
        assert!(env.is_idle(), "level {}", env.level());
        assert_eq!(env.level(), 0.0);
        assert_eq!(env.tick(), 0.0);
    }

    #[test]
    fn adsr_minimum_stage_time_is_one_millisecond() {
        let mut env = Adsr::new(SR, 0.0, 0.0, 0.5, 0.0);
        env.note_on();
        let v = env.tick();
        // Zero attack is clamped to 1 ms = 48 samples, so one tick is 1/48, not a full jump.
        assert!((v - 1.0 / 48.0).abs() < 1e-5, "first attack sample {v}");
        for _ in 0..46 {
            env.tick();
            assert_eq!(env.stage(), AdsrStage::Attack);
        }
        // 48 increments of 1/48 land on 1.0 within rounding; one extra tick covers the case
        // where the f32 sum stops a hair short.
        env.tick();
        env.tick();
        assert_eq!(env.level(), 1.0);
        // Zero release is clamped too: not idle after a single sample.
        env.note_off();
        env.tick();
        assert!(!env.is_idle());
        for _ in 0..48 {
            env.tick();
        }
        assert!(env.is_idle());

        // Negative and NaN inputs also fall back to the minimum, never panic.
        let mut weird = Adsr::new(SR, -1.0, f32::NAN, f32::INFINITY, -5.0);
        weird.note_on();
        assert!(weird.tick().is_finite());
        assert_eq!(weird.sustain(), ParamId::AmpSustain.default_value());
    }

    #[test]
    fn adsr_retrigger_continues_from_current_level() {
        let mut env = Adsr::new(SR, 0.010, 0.1, 0.8, 0.2);
        env.note_on();
        for _ in 0..samples(0.05) {
            env.tick();
        }
        env.note_off();
        for _ in 0..samples(0.02) {
            env.tick();
        }
        let before = env.level();
        assert!(before > 0.1 && before < 0.8, "mid-release level {before}");
        env.note_on();
        let after = env.tick();
        assert_eq!(env.stage(), AdsrStage::Attack);
        assert!(after > before, "retrigger must not drop the level");
        assert!(
            after - before < 0.01,
            "retrigger jumped by {}",
            after - before
        );
    }

    #[test]
    fn adsr_set_times_mid_note_keeps_level() {
        let mut env = Adsr::new(SR, 1.0, 1.0, 1.0, 1.0);
        env.note_on();
        for _ in 0..samples(0.1) {
            env.tick();
        }
        let before = env.level();
        env.set_times(SR, 0.001, 0.001, 0.2, 0.001);
        assert_eq!(env.level(), before);
        let after = env.tick();
        assert!(after > before && after - before < 0.03);
    }

    #[test]
    fn adsr_note_off_when_idle_stays_idle() {
        let mut env = Adsr::default();
        env.note_off();
        assert!(env.is_idle());
        assert_eq!(env.tick(), 0.0);
    }
}
