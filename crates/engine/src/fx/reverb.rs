//! Freeverb-style reverb: per channel, 8 parallel lowpass-feedback combs feeding 4 series
//! allpasses, with the right channel's delay lines offset by [`STEREO_SPREAD`] samples.
//!
//! The classic tunings are given for 44.1 kHz and scaled to the actual sample rate at
//! construction, so the room sounds the same size at 48 kHz or 96 kHz.
//!
//! - `size` maps to comb feedback `ROOM_OFFSET + size * ROOM_SCALE` (`0.7..=0.98`).
//! - `damping` maps to the comb lowpass coefficient `damping * DAMP_SCALE` (`0..=0.4`).
//!
//! Plugins milestone additions (all additive, defaults reproduce the M2 sound exactly):
//!
//! - **pre-delay** (`0..=`[`MAX_PRE_DELAY_MS`]): the mono input is delayed before the comb
//!   bank through a pre-allocated line with a linearly interpolated, smoothed read position.
//! - **width** (`0..=1`): mid/side blend of the two wet channels; `0` makes them identical,
//!   `1` is the full Freeverb spread.
//! - **low cut / high cut** on the wet signal: RBJ second-order filters
//!   ([`crate::fx::eq::Coeffs`]); at their extremes (20 Hz / 20 kHz) they are bypassed.
//!
//! [`ReverbState`] is the plain-data view of every parameter (serde, `Copy`) that the project
//! file stores and the UI mirrors; [`ReverbParam`] addresses one value for `Message::SetReverb`.
//! `ParamId::ReverbSize/Damping/Mix` keep working: the engine writes those into the same
//! state, so both views always agree.
//!
//! Real-time safe after construction: `process` never allocates or panics; every feedback
//! path is denormal-flushed.

use core::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::dsp::flush_denormal;
use crate::fx::eq::{BiquadState, Coeffs, EqKind};
use crate::params::Smoother;

/// Reference sample rate of the comb/allpass tunings.
pub const REFERENCE_SAMPLE_RATE: f32 = 44_100.0;

/// Comb delay lengths at 44.1 kHz (left channel).
pub const COMB_TUNINGS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];

/// Allpass delay lengths at 44.1 kHz (left channel).
pub const ALLPASS_TUNINGS: [usize; 4] = [556, 441, 341, 225];

/// Extra samples (at 44.1 kHz) added to every right-channel delay line.
pub const STEREO_SPREAD: usize = 23;

/// Input attenuation before the comb bank.
pub const FIXED_GAIN: f32 = 0.015;

/// Comb feedback at `size = 0.0`.
pub const ROOM_OFFSET: f32 = 0.7;

/// Comb feedback range covered by `size`.
pub const ROOM_SCALE: f32 = 0.28;

/// Comb lowpass coefficient range covered by `damping`.
pub const DAMP_SCALE: f32 = 0.4;

/// Allpass feedback (diffusion).
pub const ALLPASS_FEEDBACK: f32 = 0.5;

/// Gain applied to the summed wet signal.
pub const WET_GAIN: f32 = 1.0;

/// Longest pre-delay, in milliseconds (the pre-allocated line length).
pub const MAX_PRE_DELAY_MS: f32 = 200.0;

/// Smoothing time of a pre-delay change, in milliseconds (a glide, so no click).
pub const PRE_DELAY_SMOOTH_MS: f32 = 50.0;

/// Smoothing time of a width change, in milliseconds.
pub const WIDTH_SMOOTH_MS: f32 = 10.0;

/// Lowest wet low-cut frequency, Hz (the filter is bypassed there).
pub const MIN_CUT_HZ: f32 = 20.0;

/// Highest wet high-cut frequency, Hz (the filter is bypassed there).
pub const MAX_CUT_HZ: f32 = 20_000.0;

/// Width of the impulse-envelope bins, in seconds (10 ms).
pub const ENVELOPE_BIN_SECONDS: f32 = 0.01;

/// Longest decay [`ReverbState::decay_time_60db`] reports (the largest size rings for about
/// this long).
pub const MAX_DECAY_SECONDS: f32 = 15.0;

/// Scales a 44.1 kHz tuning to `sample_rate` (at least one sample).
fn scaled(len: usize, sample_rate: f32) -> usize {
    let n = (len as f32 * sample_rate / REFERENCE_SAMPLE_RATE).round();
    (n as usize).max(1)
}

/// Clamps `v` into `range`; non-finite values become `fallback`.
fn clamp_or(v: f32, range: RangeInclusive<f32>, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(*range.start(), *range.end())
    } else {
        fallback
    }
}

// ---------------------------------------------------------------------------------------------
// Parameters as data.

/// One reverb value, for `Message::SetReverb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReverbParam {
    /// Room size `0..=1` (the same value as `ParamId::ReverbSize`).
    Size,
    /// Damping `0..=1` (`ParamId::ReverbDamping`).
    Damping,
    /// Pre-delay, milliseconds `0..=200`.
    PreDelay,
    /// Stereo width `0..=1`.
    Width,
    /// Wet low cut, Hz `20..=20000` (log).
    LowCut,
    /// Wet high cut, Hz `20..=20000` (log).
    HighCut,
    /// Return level / mix `0..=1` (`ParamId::ReverbMix`).
    Mix,
}

impl ReverbParam {
    /// Every parameter, in knob order.
    pub const ALL: [ReverbParam; 7] = [
        ReverbParam::Size,
        ReverbParam::Damping,
        ReverbParam::PreDelay,
        ReverbParam::Width,
        ReverbParam::LowCut,
        ReverbParam::HighCut,
        ReverbParam::Mix,
    ];

    /// Allowed range.
    #[must_use]
    pub fn range(self) -> RangeInclusive<f32> {
        match self {
            ReverbParam::Size | ReverbParam::Damping | ReverbParam::Width | ReverbParam::Mix => {
                0.0..=1.0
            }
            ReverbParam::PreDelay => 0.0..=MAX_PRE_DELAY_MS,
            ReverbParam::LowCut | ReverbParam::HighCut => MIN_CUT_HZ..=MAX_CUT_HZ,
        }
    }

    /// Default value (size, damping and mix match the `ParamId` defaults).
    #[must_use]
    pub const fn default_value(self) -> f32 {
        match self {
            ReverbParam::Size => 0.6,
            ReverbParam::Damping => 0.4,
            ReverbParam::PreDelay => 0.0,
            ReverbParam::Width => 1.0,
            ReverbParam::LowCut => MIN_CUT_HZ,
            ReverbParam::HighCut => MAX_CUT_HZ,
            ReverbParam::Mix => 0.2,
        }
    }

    /// Caps label for the knob.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            ReverbParam::Size => "Size",
            ReverbParam::Damping => "Damp",
            ReverbParam::PreDelay => "Pre-delay",
            ReverbParam::Width => "Width",
            ReverbParam::LowCut => "Low cut",
            ReverbParam::HighCut => "High cut",
            ReverbParam::Mix => "Mix",
        }
    }

    /// `true` for the frequency parameters (log-scaled knobs).
    #[must_use]
    pub const fn is_log(self) -> bool {
        matches!(self, ReverbParam::LowCut | ReverbParam::HighCut)
    }

    /// Clamps `value` into range (non-finite values become the default).
    #[must_use]
    pub fn clamp(self, value: f32) -> f32 {
        clamp_or(value, self.range(), self.default_value())
    }
}

/// Every reverb parameter as plain data.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReverbState {
    /// Room size `0..=1`.
    pub size: f32,
    /// Damping `0..=1`.
    pub damping: f32,
    /// Pre-delay, milliseconds.
    pub pre_delay_ms: f32,
    /// Stereo width `0..=1`.
    pub width: f32,
    /// Wet low cut, Hz.
    pub low_cut_hz: f32,
    /// Wet high cut, Hz.
    pub high_cut_hz: f32,
    /// Return level / mix `0..=1`.
    pub mix: f32,
}

impl Default for ReverbState {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl ReverbState {
    /// The M2 sound: size 0.6, damping 0.4, no pre-delay, full width, no wet filtering,
    /// mix 0.2.
    pub const DEFAULT: ReverbState =
        ReverbState::new(0.6, 0.4, 0.0, 1.0, MIN_CUT_HZ, MAX_CUT_HZ, 0.2);

    /// A state with every value given (clamped lazily by [`ReverbState::normalise`]).
    #[must_use]
    pub const fn new(
        size: f32,
        damping: f32,
        pre_delay_ms: f32,
        width: f32,
        low_cut_hz: f32,
        high_cut_hz: f32,
        mix: f32,
    ) -> Self {
        Self {
            size,
            damping,
            pre_delay_ms,
            width,
            low_cut_hz,
            high_cut_hz,
            mix,
        }
    }

    /// Clamps every value into range.
    pub fn normalise(&mut self) {
        for p in ReverbParam::ALL {
            let v = self.get(p);
            self.set(p, v);
        }
    }

    /// Sets one value (clamped); returns the stored value.
    pub fn set(&mut self, param: ReverbParam, value: f32) -> f32 {
        let v = param.clamp(value);
        match param {
            ReverbParam::Size => self.size = v,
            ReverbParam::Damping => self.damping = v,
            ReverbParam::PreDelay => self.pre_delay_ms = v,
            ReverbParam::Width => self.width = v,
            ReverbParam::LowCut => self.low_cut_hz = v,
            ReverbParam::HighCut => self.high_cut_hz = v,
            ReverbParam::Mix => self.mix = v,
        }
        v
    }

    /// One value.
    #[must_use]
    pub fn get(&self, param: ReverbParam) -> f32 {
        match param {
            ReverbParam::Size => self.size,
            ReverbParam::Damping => self.damping,
            ReverbParam::PreDelay => self.pre_delay_ms,
            ReverbParam::Width => self.width,
            ReverbParam::LowCut => self.low_cut_hz,
            ReverbParam::HighCut => self.high_cut_hz,
            ReverbParam::Mix => self.mix,
        }
    }

    /// Comb feedback this size produces (`0.7..=0.98`).
    #[must_use]
    pub fn comb_feedback(&self) -> f32 {
        ROOM_OFFSET + self.size.clamp(0.0, 1.0) * ROOM_SCALE
    }

    /// Estimated time for the tail to fall 60 dB, in seconds: the longest comb's recirculation
    /// time divided by the loss per pass, shortened a little by damping, capped at
    /// [`MAX_DECAY_SECONDS`]. Pure arithmetic (no rendering), monotonic in `size`.
    #[must_use]
    pub fn decay_time_60db(&self) -> f32 {
        let g = self.comb_feedback().clamp(0.01, 0.999);
        let loop_seconds = COMB_TUNINGS[COMB_TUNINGS.len() - 1] as f32 / REFERENCE_SAMPLE_RATE;
        // Damping removes some energy each pass; treat it as extra broadband loss of up to
        // about 1.5 dB per pass at full damping (the lowpass costs the highs much more).
        let damp_loss_db = 1.5 * self.damping.clamp(0.0, 1.0);
        let loss_db = -20.0 * g.log10() + damp_loss_db;
        (loop_seconds * 60.0 / loss_db.max(1e-3)).clamp(0.01, MAX_DECAY_SECONDS)
    }

    /// Renders a unit impulse through a throwaway wet-only [`Reverb`] at `sample_rate` for
    /// `seconds` and returns the RMS of each [`ENVELOPE_BIN_SECONDS`] bin (both channels
    /// averaged), oldest first. For the UI / tests; allocates and is not real-time safe.
    #[must_use]
    pub fn impulse_envelope(&self, sample_rate: f32, seconds: f32) -> Vec<f32> {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let seconds = if seconds.is_finite() {
            seconds.clamp(ENVELOPE_BIN_SECONDS, 60.0)
        } else {
            1.0
        };
        let mut reverb = Reverb::new(sample_rate);
        reverb.apply_state(self);
        reverb.set_mix(1.0);
        reverb.reset();
        let bin_len = ((sample_rate * ENVELOPE_BIN_SECONDS).round() as usize).max(1);
        let bins = (seconds / ENVELOPE_BIN_SECONDS).ceil() as usize;
        let mut out = Vec::with_capacity(bins);
        let mut first = true;
        for _ in 0..bins {
            let mut acc = 0.0_f64;
            for _ in 0..bin_len {
                let x = if first { 1.0 } else { 0.0 };
                first = false;
                let (l, r) = reverb.process(x, x);
                acc += (f64::from(l) * f64::from(l) + f64::from(r) * f64::from(r)) * 0.5;
            }
            out.push((acc / bin_len as f64).sqrt() as f32);
        }
        out
    }
}

/// A named reverb setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReverbPreset {
    /// Display name.
    pub name: &'static str,
    /// The setting.
    pub state: ReverbState,
}

/// Built-in reverb presets (the plugin window's list).
pub const PRESETS: &[ReverbPreset] = &[
    ReverbPreset {
        name: "Small Room",
        state: ReverbState::new(0.25, 0.5, 5.0, 0.7, 120.0, 9000.0, 0.18),
    },
    ReverbPreset {
        name: "Studio",
        state: ReverbState::new(0.45, 0.45, 12.0, 0.85, 100.0, 12_000.0, 0.2),
    },
    ReverbPreset {
        name: "Plate",
        state: ReverbState::new(0.6, 0.2, 0.0, 1.0, 150.0, 15_000.0, 0.25),
    },
    ReverbPreset {
        name: "Hall",
        state: ReverbState::new(0.8, 0.4, 25.0, 1.0, 80.0, 10_000.0, 0.28),
    },
    ReverbPreset {
        name: "Cathedral",
        state: ReverbState::new(0.95, 0.35, 60.0, 1.0, 60.0, 8000.0, 0.32),
    },
    ReverbPreset {
        name: "Lofi Spring",
        state: ReverbState::new(0.5, 0.75, 30.0, 0.4, 300.0, 3500.0, 0.3),
    },
    ReverbPreset {
        name: "Dark Ambient",
        state: ReverbState::new(0.9, 0.85, 80.0, 1.0, 40.0, 2500.0, 0.4),
    },
    ReverbPreset {
        name: "Bright Chamber",
        state: ReverbState::new(0.55, 0.1, 10.0, 0.9, 200.0, 20_000.0, 0.22),
    },
    ReverbPreset {
        name: "Gated",
        state: ReverbState::new(0.3, 0.0, 0.0, 1.0, 100.0, 16_000.0, 0.35),
    },
    ReverbPreset {
        name: "Infinite Wash",
        state: ReverbState::new(1.0, 0.3, 120.0, 1.0, 100.0, 6000.0, 0.45),
    },
];

/// Index of the preset whose state equals `state`, if any.
#[must_use]
pub fn preset_index_of(state: &ReverbState) -> Option<usize> {
    PRESETS.iter().position(|p| p.state == *state)
}

// ---------------------------------------------------------------------------------------------
// DSP.

/// Lowpass-feedback comb filter.
#[derive(Debug, Clone, PartialEq)]
struct Comb {
    buf: Vec<f32>,
    idx: usize,
    filter: f32,
    feedback: f32,
    damp1: f32,
    damp2: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            idx: 0,
            filter: 0.0,
            feedback: ROOM_OFFSET,
            damp1: 0.0,
            damp2: 1.0,
        }
    }

    fn set(&mut self, feedback: f32, damp: f32) {
        self.feedback = feedback;
        self.damp1 = damp;
        self.damp2 = 1.0 - damp;
    }

    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.idx = 0;
        self.filter = 0.0;
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let out = self.buf[self.idx];
        self.filter = flush_denormal(out * self.damp2 + self.filter * self.damp1);
        self.buf[self.idx] = flush_denormal(input + self.filter * self.feedback);
        self.idx += 1;
        if self.idx == self.buf.len() {
            self.idx = 0;
        }
        out
    }
}

/// Schroeder allpass with fixed feedback.
#[derive(Debug, Clone, PartialEq)]
struct Allpass {
    buf: Vec<f32>,
    idx: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            idx: 0,
        }
    }

    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.idx = 0;
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let bufout = self.buf[self.idx];
        self.buf[self.idx] = flush_denormal(input + bufout * ALLPASS_FEEDBACK);
        self.idx += 1;
        if self.idx == self.buf.len() {
            self.idx = 0;
        }
        bufout - input
    }
}

/// One channel of the reverb network.
#[derive(Debug, Clone, PartialEq)]
struct Channel {
    combs: [Comb; 8],
    allpasses: [Allpass; 4],
}

impl Channel {
    fn new(sample_rate: f32, spread: usize) -> Self {
        Self {
            combs: core::array::from_fn(|i| {
                Comb::new(scaled(COMB_TUNINGS[i] + spread, sample_rate))
            }),
            allpasses: core::array::from_fn(|i| {
                Allpass::new(scaled(ALLPASS_TUNINGS[i] + spread, sample_rate))
            }),
        }
    }

    fn set(&mut self, feedback: f32, damp: f32) {
        for c in &mut self.combs {
            c.set(feedback, damp);
        }
    }

    fn reset(&mut self) {
        for c in &mut self.combs {
            c.reset();
        }
        for a in &mut self.allpasses {
            a.reset();
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let mut out = 0.0;
        for c in &mut self.combs {
            out += c.process(input);
        }
        for a in &mut self.allpasses {
            out = a.process(out);
        }
        out
    }
}

/// A second-order wet filter (low or high cut) on both channels, bypassed at its extreme.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WetFilter {
    kind: EqKind,
    hz: f32,
    active: bool,
    coeffs: Coeffs,
    left: BiquadState,
    right: BiquadState,
}

impl WetFilter {
    fn new(kind: EqKind, hz: f32, sample_rate: f32) -> Self {
        let mut f = Self {
            kind,
            hz,
            active: false,
            coeffs: Coeffs::IDENTITY,
            left: BiquadState::default(),
            right: BiquadState::default(),
        };
        f.set(hz, sample_rate);
        f
    }

    fn set(&mut self, hz: f32, sample_rate: f32) {
        let hz = clamp_or(hz, MIN_CUT_HZ..=MAX_CUT_HZ, self.hz);
        self.hz = hz;
        self.active = match self.kind {
            EqKind::LowCut => hz > MIN_CUT_HZ,
            _ => hz < MAX_CUT_HZ,
        };
        if self.active {
            self.coeffs = Coeffs::compute(
                self.kind,
                hz,
                0.0,
                core::f32::consts::FRAC_1_SQRT_2,
                sample_rate,
            );
        } else {
            self.left.reset();
            self.right.reset();
        }
    }

    fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        if self.active {
            (
                self.left.tick(l, &self.coeffs),
                self.right.tick(r, &self.coeffs),
            )
        } else {
            (l, r)
        }
    }
}

/// Stereo Freeverb with pre-delay, width and wet filters.
#[derive(Debug, Clone, PartialEq)]
pub struct Reverb {
    left: Channel,
    right: Channel,
    size: f32,
    damping: f32,
    mix: f32,
    sample_rate: f32,
    /// Pre-delay line (mono input), `MAX_PRE_DELAY_MS` plus a guard sample.
    pre_buf: Vec<f32>,
    pre_idx: usize,
    /// Pre-delay in samples, smoothed.
    pre_delay: Smoother,
    pre_delay_ms: f32,
    width: Smoother,
    low_cut: WetFilter,
    high_cut: WetFilter,
}

impl Reverb {
    /// Pre-allocates the delay lines for `sample_rate` (non-finite or non-positive rates fall
    /// back to 48 kHz). Starts with size `0.0`, damping `0.0`, a fully dry mix, no pre-delay,
    /// full width and no wet filtering.
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let pre_len = (sample_rate * MAX_PRE_DELAY_MS * 0.001).ceil() as usize + 2;
        let mut r = Self {
            left: Channel::new(sample_rate, 0),
            right: Channel::new(sample_rate, STEREO_SPREAD),
            size: -1.0,
            damping: -1.0,
            mix: 0.0,
            sample_rate,
            pre_buf: vec![0.0; pre_len],
            pre_idx: 0,
            pre_delay: Smoother::new(sample_rate, PRE_DELAY_SMOOTH_MS, 0.0),
            pre_delay_ms: 0.0,
            width: Smoother::new(sample_rate, WIDTH_SMOOTH_MS, 1.0),
            low_cut: WetFilter::new(EqKind::LowCut, MIN_CUT_HZ, sample_rate),
            high_cut: WetFilter::new(EqKind::HighCut, MAX_CUT_HZ, sample_rate),
        };
        r.set_size(0.0);
        r.set_damping(0.0);
        r
    }

    /// Sample rate the delay lines were scaled for.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    fn update(&mut self) {
        let feedback = ROOM_OFFSET + self.size * ROOM_SCALE;
        let damp = self.damping * DAMP_SCALE;
        self.left.set(feedback, damp);
        self.right.set(feedback, damp);
    }

    /// Sets the room size in `0.0..=1.0` (clamped; non-finite ignored). Cheap when unchanged.
    pub fn set_size(&mut self, size: f32) {
        if !size.is_finite() {
            return;
        }
        let size = size.clamp(0.0, 1.0);
        if size != self.size {
            self.size = size;
            self.update();
        }
    }

    /// Sets the damping in `0.0..=1.0` (clamped; non-finite ignored). Cheap when unchanged.
    pub fn set_damping(&mut self, damping: f32) {
        if !damping.is_finite() {
            return;
        }
        let damping = damping.clamp(0.0, 1.0);
        if damping != self.damping {
            self.damping = damping;
            self.update();
        }
    }

    /// Sets the dry/wet mix in `0.0..=1.0` (clamped; non-finite ignored).
    pub fn set_mix(&mut self, mix: f32) {
        if mix.is_finite() {
            self.mix = mix.clamp(0.0, 1.0);
        }
    }

    /// Sets the pre-delay in milliseconds (`0..=`[`MAX_PRE_DELAY_MS`], clamped; non-finite
    /// ignored). The read position glides there over [`PRE_DELAY_SMOOTH_MS`].
    pub fn set_pre_delay_ms(&mut self, ms: f32) {
        if !ms.is_finite() {
            return;
        }
        self.pre_delay_ms = ms.clamp(0.0, MAX_PRE_DELAY_MS);
        let samples = self.pre_delay_ms * 0.001 * self.sample_rate;
        self.pre_delay
            .set_target(samples.min((self.pre_buf.len() - 2) as f32));
    }

    /// Sets the stereo width in `0.0..=1.0` (clamped; non-finite ignored), smoothed.
    pub fn set_width(&mut self, width: f32) {
        if width.is_finite() {
            self.width.set_target(width.clamp(0.0, 1.0));
        }
    }

    /// Sets the wet low-cut frequency, Hz (`20` bypasses the filter).
    pub fn set_low_cut_hz(&mut self, hz: f32) {
        let sr = self.sample_rate;
        self.low_cut.set(hz, sr);
    }

    /// Sets the wet high-cut frequency, Hz (`20000` bypasses the filter).
    pub fn set_high_cut_hz(&mut self, hz: f32) {
        let sr = self.sample_rate;
        self.high_cut.set(hz, sr);
    }

    /// Applies every value of `state` (size, damping, mix included).
    pub fn apply_state(&mut self, state: &ReverbState) {
        let mut s = *state;
        s.normalise();
        self.set_size(s.size);
        self.set_damping(s.damping);
        self.set_mix(s.mix);
        self.set_pre_delay_ms(s.pre_delay_ms);
        self.set_width(s.width);
        self.set_low_cut_hz(s.low_cut_hz);
        self.set_high_cut_hz(s.high_cut_hz);
    }

    /// Every current target value as a [`ReverbState`].
    #[must_use]
    pub fn state(&self) -> ReverbState {
        ReverbState::new(
            self.size,
            self.damping,
            self.pre_delay_ms,
            self.width.target(),
            self.low_cut.hz,
            self.high_cut.hz,
            self.mix,
        )
    }

    /// Current room size.
    #[must_use]
    pub fn size(&self) -> f32 {
        self.size
    }

    /// Current damping.
    #[must_use]
    pub fn damping(&self) -> f32 {
        self.damping
    }

    /// Current mix.
    #[must_use]
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Current pre-delay target, milliseconds.
    #[must_use]
    pub fn pre_delay_ms(&self) -> f32 {
        self.pre_delay_ms
    }

    /// Current width target.
    #[must_use]
    pub fn width(&self) -> f32 {
        self.width.target()
    }

    /// Current wet low-cut frequency, Hz.
    #[must_use]
    pub fn low_cut_hz(&self) -> f32 {
        self.low_cut.hz
    }

    /// Current wet high-cut frequency, Hz.
    #[must_use]
    pub fn high_cut_hz(&self) -> f32 {
        self.high_cut.hz
    }

    /// Comb feedback currently in use (`0.7..=0.98`).
    #[must_use]
    pub fn comb_feedback(&self) -> f32 {
        ROOM_OFFSET + self.size * ROOM_SCALE
    }

    /// Clears every delay line (kills the tail instantly) and snaps the pre-delay and width
    /// smoothers to their targets.
    pub fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
        self.pre_buf.fill(0.0);
        self.pre_idx = 0;
        self.pre_delay.snap(self.pre_delay.target());
        self.width.snap(self.width.target());
        self.low_cut.reset();
        self.high_cut.reset();
    }

    /// Writes `input` into the pre-delay line and returns the sample `delay` samples back
    /// (linear interpolation; `0.0` returns `input` itself).
    #[inline]
    fn pre_delay_tick(&mut self, input: f32) -> f32 {
        let len = self.pre_buf.len();
        self.pre_buf[self.pre_idx] = input;
        let delay = self.pre_delay.tick().clamp(0.0, (len - 2) as f32);
        let whole = delay as usize;
        let frac = delay - whole as f32;
        let i0 = (self.pre_idx + len - whole) % len;
        let i1 = (i0 + len - 1) % len;
        let out = self.pre_buf[i0] + (self.pre_buf[i1] - self.pre_buf[i0]) * frac;
        self.pre_idx += 1;
        if self.pre_idx == len {
            self.pre_idx = 0;
        }
        out
    }

    /// Processes one stereo frame. Non-finite inputs are treated as `0.0`.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let l = if l.is_finite() { l } else { 0.0 };
        let r = if r.is_finite() { r } else { 0.0 };
        let input = self.pre_delay_tick((l + r) * FIXED_GAIN);
        let wet_l = self.left.process(input) * WET_GAIN;
        let wet_r = self.right.process(input) * WET_GAIN;
        let w = self.width.tick();
        let (wet_l, wet_r) = if w >= 1.0 {
            (wet_l, wet_r)
        } else {
            let mid = (wet_l + wet_r) * 0.5;
            let side = (wet_l - wet_r) * 0.5 * w;
            (mid + side, mid - side)
        };
        let (wet_l, wet_r) = self.low_cut.process(wet_l, wet_r);
        let (wet_l, wet_r) = self.high_cut.process(wet_l, wet_r);
        (l + (wet_l - l) * self.mix, r + (wet_r - r) * self.mix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn tail(rev: &mut Reverb, len: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            let x = if i == 0 { 1.0 } else { 0.0 };
            out.push(rev.process(x, x).0);
        }
        out
    }

    fn energy(buf: &[f32]) -> f64 {
        buf.iter().map(|&v| f64::from(v) * f64::from(v)).sum()
    }

    #[test]
    fn tunings_scale_with_sample_rate() {
        assert_eq!(scaled(1116, 44_100.0), 1116);
        assert_eq!(scaled(1116, 88_200.0), 2232);
        assert_eq!(scaled(1116, 48_000.0), 1215);
        let r = Reverb::new(SR);
        assert_eq!(r.left.combs[0].buf.len(), 1215);
        assert_eq!(r.right.combs[0].buf.len(), scaled(1116 + 23, SR));
        assert_eq!(r.left.allpasses[3].buf.len(), scaled(225, SR));
    }

    #[test]
    fn size_maps_to_comb_feedback() {
        let mut r = Reverb::new(SR);
        assert!((r.comb_feedback() - 0.7).abs() < 1e-6);
        r.set_size(1.0);
        assert!((r.comb_feedback() - 0.98).abs() < 1e-6);
        r.set_size(5.0);
        assert_eq!(r.size(), 1.0);
        r.set_damping(1.0);
        assert!((r.left.combs[0].damp1 - 0.4).abs() < 1e-6);
        r.set_damping(f32::NAN);
        assert_eq!(r.damping(), 1.0);
    }

    #[test]
    fn bigger_room_rings_longer() {
        let n = (SR * 2.0) as usize;
        let after = (SR * 0.5) as usize;
        let mut small = Reverb::new(SR);
        small.set_size(0.1);
        small.set_mix(1.0);
        let mut big = Reverb::new(SR);
        big.set_size(0.9);
        big.set_mix(1.0);
        let ts = tail(&mut small, n);
        let tb = tail(&mut big, n);
        assert!(ts.iter().chain(tb.iter()).all(|v| v.is_finite()));
        assert!(energy(&tb[after..]) > energy(&ts[after..]) * 10.0);
    }

    #[test]
    fn dry_mix_is_identity_and_reset_kills_tail() {
        let mut r = Reverb::new(SR);
        r.set_size(1.0);
        r.set_mix(0.0);
        assert_eq!(r.process(0.4, -0.2), (0.4, -0.2));
        r.set_mix(1.0);
        r.reset();
        for _ in 0..10_000 {
            assert_eq!(r.process(0.0, 0.0), (0.0, 0.0));
        }
    }

    #[test]
    fn non_finite_input_is_ignored() {
        let mut r = Reverb::new(SR);
        r.set_mix(1.0);
        assert_eq!(r.process(f32::NAN, f32::NEG_INFINITY), (0.0, 0.0));
        for _ in 0..5000 {
            assert_eq!(r.process(0.0, 0.0), (0.0, 0.0));
        }
    }

    #[test]
    fn state_round_trips_and_clamps() {
        let mut s = ReverbState::new(2.0, -1.0, 999.0, 1.5, 1.0, 99_999.0, f32::NAN);
        s.normalise();
        assert_eq!(
            s,
            ReverbState::new(1.0, 0.0, 200.0, 1.0, 20.0, 20_000.0, 0.2)
        );
        for p in ReverbParam::ALL {
            assert_eq!(ReverbState::DEFAULT.get(p), p.default_value(), "{p:?}");
            assert!(!p.label().is_empty());
        }
        let mut r = Reverb::new(SR);
        r.apply_state(&PRESETS[3].state);
        assert_eq!(r.state(), PRESETS[3].state);
        assert!(PRESETS.len() >= 10);
        assert_eq!(preset_index_of(&PRESETS[3].state), Some(3));
        assert_eq!(preset_index_of(&ReverbState::DEFAULT), None);
    }

    #[test]
    fn default_state_is_the_m2_reverb() {
        // With the default state applied the extensions are all pass-through.
        let mut plain = Reverb::new(SR);
        plain.set_size(0.6);
        plain.set_damping(0.4);
        plain.set_mix(0.2);
        let mut extended = Reverb::new(SR);
        extended.apply_state(&ReverbState::DEFAULT);
        extended.reset();
        for i in 0..20_000 {
            let x = ((i as f32) * 0.03).sin() * 0.4;
            assert_eq!(
                plain.process(x, -x * 0.5),
                extended.process(x, -x * 0.5),
                "{i}"
            );
        }
    }

    #[test]
    fn decay_time_grows_with_size() {
        let mut last = 0.0;
        for i in 0..=10 {
            let s = ReverbState::new(i as f32 / 10.0, 0.4, 0.0, 1.0, 20.0, 20_000.0, 1.0);
            let t = s.decay_time_60db();
            assert!(t > last, "size {i}: {t} > {last}");
            last = t;
        }
        assert!(last <= MAX_DECAY_SECONDS);
        let damped = ReverbState::new(0.5, 1.0, 0.0, 1.0, 20.0, 20_000.0, 1.0);
        let open = ReverbState::new(0.5, 0.0, 0.0, 1.0, 20.0, 20_000.0, 1.0);
        assert!(damped.decay_time_60db() < open.decay_time_60db());
    }
}
