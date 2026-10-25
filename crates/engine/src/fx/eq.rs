//! Seven-band parametric EQ on the master bus (plugins milestone).
//!
//! [`EqState`] is the plain data (serde, `Copy`): seven [`EqBand`]s, a master `on` switch and
//! an output gain. [`ParametricEq`] is the DSP: one RBJ cookbook biquad per band and channel
//! in transposed direct form II, with the band's frequency and gain smoothed over about
//! [`EQ_SMOOTH_MS`] so a knob move or a node drag never zippers. Coefficients are recomputed
//! only while a band's smoothers are moving (or when its kind / Q / on state changes), never
//! per sample in the steady state. Every feedback path is denormal-flushed.
//!
//! The engine runs the EQ after the mixer sum and the effect returns and before the master
//! gain and the limiter (`crate::engine`). A flat state (every shelf / peak at 0 dB, the cuts
//! off, output gain 0 dB) is bit-transparent: the identity biquads reproduce the input
//! exactly, so existing level tests are unaffected.
//!
//! [`EqState::response_db`] is the magnitude of the whole chain at one frequency, for the UI
//! curve; it uses the target (unsmoothed) values and is cheap enough to evaluate a few
//! hundred times per frame on the UI thread.

use core::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::dsp::{db_to_gain, flush_denormal};
use crate::params::{DEFAULT_SMOOTH_MS, Smoother};

/// Number of bands.
pub const BAND_COUNT: usize = 7;
/// Lowest band frequency, Hz.
pub const MIN_FREQ: f32 = 20.0;
/// Highest band frequency, Hz.
pub const MAX_FREQ: f32 = 20_000.0;
/// Lowest band / output gain, dB.
pub const MIN_GAIN_DB: f32 = -18.0;
/// Highest band / output gain, dB.
pub const MAX_GAIN_DB: f32 = 18.0;
/// Lowest Q.
pub const MIN_Q: f32 = 0.1;
/// Highest Q.
pub const MAX_Q: f32 = 10.0;
/// Q of a band at its default (Butterworth).
pub const DEFAULT_Q: f32 = 0.707;
/// Smoothing time constant of the band frequency / gain, the band on/off fades, the master
/// on/off fade and the output gain, in milliseconds.
pub const EQ_SMOOTH_MS: f32 = DEFAULT_SMOOTH_MS;
/// Band frequencies are clamped below this fraction of the sample rate for stability.
pub const MAX_FREQ_FRACTION: f32 = 0.49;

/// Filter shape of a band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EqKind {
    /// Second-order high-pass (12 dB/oct); `gain` is ignored.
    LowCut,
    /// Low shelf: boosts / cuts everything below `freq` by `gain`.
    LowShelf,
    /// Peaking bell: `gain` at `freq`, width from `q`.
    #[default]
    Peak,
    /// High shelf: boosts / cuts everything above `freq` by `gain`.
    HighShelf,
    /// Second-order low-pass (12 dB/oct); `gain` is ignored.
    HighCut,
}

impl EqKind {
    /// Every kind, in the order of the UI's TYPE selector.
    pub const ALL: [EqKind; 5] = [
        EqKind::LowCut,
        EqKind::LowShelf,
        EqKind::Peak,
        EqKind::HighShelf,
        EqKind::HighCut,
    ];

    /// Index in [`EqKind::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Kind at `index` (clamped to the last).
    #[must_use]
    pub const fn from_index(index: usize) -> EqKind {
        if index >= Self::ALL.len() {
            Self::ALL[Self::ALL.len() - 1]
        } else {
            Self::ALL[index]
        }
    }

    /// Kind for a parameter value (`SetEq` with [`EqParam::Kind`]): rounded, clamped.
    #[must_use]
    pub fn from_value(value: f32) -> EqKind {
        if value.is_finite() {
            Self::from_index(value.round().max(0.0) as usize)
        } else {
            EqKind::Peak
        }
    }

    /// Short label for the TYPE selector.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            EqKind::LowCut => "Low cut",
            EqKind::LowShelf => "Low shelf",
            EqKind::Peak => "Peak",
            EqKind::HighShelf => "High shelf",
            EqKind::HighCut => "High cut",
        }
    }

    /// `true` for the kinds whose `gain` matters (shelves and the peak).
    #[must_use]
    pub const fn has_gain(self) -> bool {
        !matches!(self, EqKind::LowCut | EqKind::HighCut)
    }
}

/// One value of a band, for `Message::SetEq`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EqParam {
    /// Band enabled: `0` off, anything `>= 0.5` on.
    On,
    /// [`EqKind`] index (see [`EqKind::from_value`]).
    Kind,
    /// Centre / corner frequency, Hz (`MIN_FREQ..=MAX_FREQ`).
    Freq,
    /// Gain, dB (`MIN_GAIN_DB..=MAX_GAIN_DB`).
    Gain,
    /// Quality factor (`MIN_Q..=MAX_Q`).
    Q,
}

impl EqParam {
    /// Every band parameter.
    pub const ALL: [EqParam; 5] = [
        EqParam::On,
        EqParam::Kind,
        EqParam::Freq,
        EqParam::Gain,
        EqParam::Q,
    ];

    /// Allowed range.
    #[must_use]
    pub fn range(self) -> RangeInclusive<f32> {
        match self {
            EqParam::On => 0.0..=1.0,
            EqParam::Kind => 0.0..=(EqKind::ALL.len() - 1) as f32,
            EqParam::Freq => MIN_FREQ..=MAX_FREQ,
            EqParam::Gain => MIN_GAIN_DB..=MAX_GAIN_DB,
            EqParam::Q => MIN_Q..=MAX_Q,
        }
    }

    /// Caps label for the band controls.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            EqParam::On => "On",
            EqParam::Kind => "Type",
            EqParam::Freq => "Freq",
            EqParam::Gain => "Gain",
            EqParam::Q => "Q",
        }
    }
}

/// Clamps `v` into `range`; non-finite values become `fallback`.
fn clamp_or(v: f32, range: RangeInclusive<f32>, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(*range.start(), *range.end())
    } else {
        fallback
    }
}

/// One EQ band.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EqBand {
    /// Whether the band processes audio.
    pub on: bool,
    /// Filter shape.
    pub kind: EqKind,
    /// Centre / corner frequency, Hz.
    pub freq: f32,
    /// Gain, dB (ignored by the cuts).
    pub gain_db: f32,
    /// Quality factor.
    pub q: f32,
}

impl Default for EqBand {
    /// A peak at 1 kHz, 0 dB, Q 0.707, on.
    fn default() -> Self {
        Self::new(EqKind::Peak, 1000.0, 0.0, DEFAULT_Q, true)
    }
}

impl EqBand {
    /// A band with every value given (clamped lazily by [`EqBand::normalise`]).
    #[must_use]
    pub const fn new(kind: EqKind, freq: f32, gain_db: f32, q: f32, on: bool) -> Self {
        Self {
            on,
            kind,
            freq,
            gain_db,
            q,
        }
    }

    /// Clamps every value into range (non-finite values go to the defaults).
    pub fn normalise(&mut self) {
        self.freq = clamp_or(self.freq, EqParam::Freq.range(), 1000.0);
        self.gain_db = clamp_or(self.gain_db, EqParam::Gain.range(), 0.0);
        self.q = clamp_or(self.q, EqParam::Q.range(), DEFAULT_Q);
    }

    /// Sets one value (clamped) and returns the stored value.
    pub fn set(&mut self, param: EqParam, value: f32) -> f32 {
        match param {
            EqParam::On => {
                self.on = value.is_finite() && value >= 0.5;
                f32::from(u8::from(self.on))
            }
            EqParam::Kind => {
                self.kind = EqKind::from_value(value);
                self.kind.index() as f32
            }
            EqParam::Freq => {
                self.freq = clamp_or(value, EqParam::Freq.range(), self.freq);
                self.freq
            }
            EqParam::Gain => {
                self.gain_db = clamp_or(value, EqParam::Gain.range(), self.gain_db);
                self.gain_db
            }
            EqParam::Q => {
                self.q = clamp_or(value, EqParam::Q.range(), self.q);
                self.q
            }
        }
    }

    /// One value as the `f32` the messages carry.
    #[must_use]
    pub fn get(&self, param: EqParam) -> f32 {
        match param {
            EqParam::On => f32::from(u8::from(self.on)),
            EqParam::Kind => self.kind.index() as f32,
            EqParam::Freq => self.freq,
            EqParam::Gain => self.gain_db,
            EqParam::Q => self.q,
        }
    }

    /// Magnitude response of this band alone at `freq_hz`, in dB (0 when off).
    #[must_use]
    pub fn response_db(&self, freq_hz: f32, sample_rate: f32) -> f32 {
        if !self.on {
            return 0.0;
        }
        Coeffs::compute(self.kind, self.freq, self.gain_db, self.q, sample_rate)
            .response_db(freq_hz, sample_rate)
    }
}

/// The whole EQ: seven bands, the master switch and the output gain.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EqState {
    /// The bands, low to high in the classic layout.
    pub bands: [EqBand; BAND_COUNT],
    /// Master switch; off = bit-transparent bypass (faded over [`EQ_SMOOTH_MS`]).
    pub on: bool,
    /// Output gain after the bands, dB.
    pub gain_db: f32,
}

impl Default for EqState {
    /// The flat classic layout ([`EqState::FLAT`]).
    fn default() -> Self {
        Self::FLAT
    }
}

impl EqState {
    /// The classic layout, flat: low cut 30 Hz (off), low shelf 100 Hz, peaks at 250 Hz, 1 kHz
    /// and 4 kHz, high shelf 10 kHz, high cut 18 kHz (off); EQ on, output 0 dB.
    pub const FLAT: EqState =
        EqState::layout([0.0; 5], [false, true, true, true, true, true, false]);

    /// The classic layout's frequencies, low to high.
    pub const LAYOUT_FREQS: [f32; BAND_COUNT] =
        [30.0, 100.0, 250.0, 1000.0, 4000.0, 10_000.0, 18_000.0];

    /// The classic layout's kinds, low to high.
    pub const LAYOUT_KINDS: [EqKind; BAND_COUNT] = [
        EqKind::LowCut,
        EqKind::LowShelf,
        EqKind::Peak,
        EqKind::Peak,
        EqKind::Peak,
        EqKind::HighShelf,
        EqKind::HighCut,
    ];

    /// The classic layout with the five gain bands (low shelf, three peaks, high shelf) at
    /// `gains` dB and the given on flags; Q 0.707 everywhere.
    #[must_use]
    pub const fn layout(gains: [f32; 5], on: [bool; BAND_COUNT]) -> EqState {
        EqState {
            bands: [
                EqBand::new(EqKind::LowCut, 30.0, 0.0, DEFAULT_Q, on[0]),
                EqBand::new(EqKind::LowShelf, 100.0, gains[0], DEFAULT_Q, on[1]),
                EqBand::new(EqKind::Peak, 250.0, gains[1], DEFAULT_Q, on[2]),
                EqBand::new(EqKind::Peak, 1000.0, gains[2], DEFAULT_Q, on[3]),
                EqBand::new(EqKind::Peak, 4000.0, gains[3], DEFAULT_Q, on[4]),
                EqBand::new(EqKind::HighShelf, 10_000.0, gains[4], DEFAULT_Q, on[5]),
                EqBand::new(EqKind::HighCut, 18_000.0, 0.0, DEFAULT_Q, on[6]),
            ],
            on: true,
            gain_db: 0.0,
        }
    }

    /// Clamps every value into range.
    pub fn normalise(&mut self) {
        for b in &mut self.bands {
            b.normalise();
        }
        self.gain_db = clamp_or(self.gain_db, EqParam::Gain.range(), 0.0);
    }

    /// Sets one value of band `band` (wrapped into `0..BAND_COUNT`); returns the stored value.
    pub fn set(&mut self, band: usize, param: EqParam, value: f32) -> f32 {
        self.bands[band % BAND_COUNT].set(param, value)
    }

    /// The band at `index` (wrapped).
    #[must_use]
    pub fn band(&self, index: usize) -> &EqBand {
        &self.bands[index % BAND_COUNT]
    }

    /// Sets the output gain (dB, clamped); returns the stored value.
    pub fn set_gain_db(&mut self, gain_db: f32) -> f32 {
        self.gain_db = clamp_or(gain_db, EqParam::Gain.range(), self.gain_db);
        self.gain_db
    }

    /// Magnitude of the whole EQ at `freq_hz` in dB: the sum of the enabled bands' responses
    /// plus the output gain, or exactly `0.0` when the EQ is off.
    #[must_use]
    pub fn response_db(&self, freq_hz: f32, sample_rate: f32) -> f32 {
        if !self.on {
            return 0.0;
        }
        self.bands
            .iter()
            .map(|b| b.response_db(freq_hz, sample_rate))
            .sum::<f32>()
            + self.gain_db
    }

    /// `true` when every enabled band is flat (0 dB shelves / peaks, no cuts) and the output
    /// gain is 0 dB, i.e. the EQ changes nothing.
    #[must_use]
    pub fn is_flat(&self) -> bool {
        self.gain_db == 0.0
            && self
                .bands
                .iter()
                .all(|b| !b.on || (b.kind.has_gain() && b.gain_db == 0.0))
    }
}

/// A named EQ setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqPreset {
    /// Display name.
    pub name: &'static str,
    /// The setting.
    pub state: EqState,
}

const ON_GAINS: [bool; BAND_COUNT] = [false, true, true, true, true, true, false];

/// Built-in EQ presets (the plugin window's list).
pub const PRESETS: &[EqPreset] = &[
    EqPreset {
        name: "Flat",
        state: EqState::FLAT,
    },
    EqPreset {
        name: "Bass Boost",
        state: EqState::layout([6.0, 2.0, 0.0, 0.0, 0.0], ON_GAINS),
    },
    EqPreset {
        name: "Sub Cut",
        state: EqState {
            bands: [
                EqBand::new(EqKind::LowCut, 40.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::LowShelf, 100.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 250.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 1000.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 4000.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::HighShelf, 10_000.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::HighCut, 18_000.0, 0.0, DEFAULT_Q, false),
            ],
            on: true,
            gain_db: 0.0,
        },
    },
    EqPreset {
        name: "Lofi Telephone",
        state: EqState {
            bands: [
                EqBand::new(EqKind::LowCut, 400.0, 0.0, 0.8, true),
                EqBand::new(EqKind::LowShelf, 600.0, -6.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 1200.0, 4.0, 1.2, true),
                EqBand::new(EqKind::Peak, 2000.0, 6.0, 1.5, true),
                EqBand::new(EqKind::Peak, 3000.0, 2.0, 1.0, true),
                EqBand::new(EqKind::HighShelf, 4000.0, -9.0, DEFAULT_Q, true),
                EqBand::new(EqKind::HighCut, 3600.0, 0.0, 0.8, true),
            ],
            on: true,
            gain_db: 2.0,
        },
    },
    EqPreset {
        name: "Air",
        state: EqState {
            bands: [
                EqBand::new(EqKind::LowCut, 30.0, 0.0, DEFAULT_Q, false),
                EqBand::new(EqKind::LowShelf, 100.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 250.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 1000.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 4000.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::HighShelf, 12_000.0, 5.0, 0.6, true),
                EqBand::new(EqKind::HighCut, 18_000.0, 0.0, DEFAULT_Q, false),
            ],
            on: true,
            gain_db: 0.0,
        },
    },
    EqPreset {
        name: "De-Mud",
        state: EqState {
            bands: [
                EqBand::new(EqKind::LowCut, 30.0, 0.0, DEFAULT_Q, false),
                EqBand::new(EqKind::LowShelf, 100.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 300.0, -5.0, 1.4, true),
                EqBand::new(EqKind::Peak, 1000.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 4000.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::HighShelf, 10_000.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::HighCut, 18_000.0, 0.0, DEFAULT_Q, false),
            ],
            on: true,
            gain_db: 0.0,
        },
    },
    EqPreset {
        name: "Presence",
        state: EqState {
            bands: [
                EqBand::new(EqKind::LowCut, 30.0, 0.0, DEFAULT_Q, false),
                EqBand::new(EqKind::LowShelf, 100.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 250.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 1000.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 3500.0, 4.5, 1.0, true),
                EqBand::new(EqKind::HighShelf, 10_000.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::HighCut, 18_000.0, 0.0, DEFAULT_Q, false),
            ],
            on: true,
            gain_db: 0.0,
        },
    },
    EqPreset {
        name: "Radio",
        state: EqState {
            bands: [
                EqBand::new(EqKind::LowCut, 120.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::LowShelf, 200.0, -3.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 500.0, 2.0, 0.9, true),
                EqBand::new(EqKind::Peak, 1500.0, 3.0, 1.0, true),
                EqBand::new(EqKind::Peak, 4000.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::HighShelf, 8000.0, -4.0, DEFAULT_Q, true),
                EqBand::new(EqKind::HighCut, 9000.0, 0.0, DEFAULT_Q, true),
            ],
            on: true,
            gain_db: 1.0,
        },
    },
    EqPreset {
        name: "Warm Tape",
        state: EqState {
            bands: [
                EqBand::new(EqKind::LowCut, 30.0, 0.0, DEFAULT_Q, false),
                EqBand::new(EqKind::LowShelf, 120.0, 2.5, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 250.0, 1.0, 0.8, true),
                EqBand::new(EqKind::Peak, 1000.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 4000.0, -1.5, 0.9, true),
                EqBand::new(EqKind::HighShelf, 9000.0, -4.0, DEFAULT_Q, true),
                EqBand::new(EqKind::HighCut, 15_000.0, 0.0, DEFAULT_Q, true),
            ],
            on: true,
            gain_db: 0.0,
        },
    },
    EqPreset {
        name: "Club Low End",
        state: EqState {
            bands: [
                EqBand::new(EqKind::LowCut, 28.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::LowShelf, 90.0, 4.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 60.0, 3.0, 1.2, true),
                EqBand::new(EqKind::Peak, 1000.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 4000.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::HighShelf, 10_000.0, 1.5, DEFAULT_Q, true),
                EqBand::new(EqKind::HighCut, 18_000.0, 0.0, DEFAULT_Q, false),
            ],
            on: true,
            gain_db: -1.0,
        },
    },
    EqPreset {
        name: "Scooped",
        state: EqState {
            bands: [
                EqBand::new(EqKind::LowCut, 30.0, 0.0, DEFAULT_Q, false),
                EqBand::new(EqKind::LowShelf, 120.0, 4.0, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 400.0, -3.0, 0.8, true),
                EqBand::new(EqKind::Peak, 900.0, -6.0, 0.7, true),
                EqBand::new(EqKind::Peak, 4000.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::HighShelf, 8000.0, 4.0, DEFAULT_Q, true),
                EqBand::new(EqKind::HighCut, 18_000.0, 0.0, DEFAULT_Q, false),
            ],
            on: true,
            gain_db: 0.0,
        },
    },
    EqPreset {
        name: "Vocal Clarity",
        state: EqState {
            bands: [
                EqBand::new(EqKind::LowCut, 90.0, 0.0, DEFAULT_Q, true),
                EqBand::new(EqKind::LowShelf, 150.0, -1.5, DEFAULT_Q, true),
                EqBand::new(EqKind::Peak, 300.0, -2.0, 1.0, true),
                EqBand::new(EqKind::Peak, 2500.0, 3.0, 1.0, true),
                EqBand::new(EqKind::Peak, 5000.0, 2.0, 1.2, true),
                EqBand::new(EqKind::HighShelf, 11_000.0, 2.5, DEFAULT_Q, true),
                EqBand::new(EqKind::HighCut, 18_000.0, 0.0, DEFAULT_Q, false),
            ],
            on: true,
            gain_db: 0.0,
        },
    },
];

/// Index of the preset whose state equals `state`, if any.
#[must_use]
pub fn preset_index_of(state: &EqState) -> Option<usize> {
    PRESETS.iter().position(|p| p.state == *state)
}

// ---------------------------------------------------------------------------------------------
// Biquads.

/// Normalised biquad coefficients (`a0 == 1`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Coeffs {
    /// The identity filter.
    pub const IDENTITY: Coeffs = Coeffs {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    /// RBJ cookbook coefficients for `kind` at `freq` Hz (clamped below
    /// [`MAX_FREQ_FRACTION`] of `sample_rate`), `gain_db` and `q`. Computed in `f64`.
    #[must_use]
    pub fn compute(kind: EqKind, freq: f32, gain_db: f32, q: f32, sample_rate: f32) -> Coeffs {
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
            f64::from(sample_rate)
        } else {
            48_000.0
        };
        let freq = f64::from(clamp_or(freq, EqParam::Freq.range(), 1000.0))
            .min(sr * f64::from(MAX_FREQ_FRACTION));
        let q = f64::from(clamp_or(q, EqParam::Q.range(), DEFAULT_Q));
        let gain_db = f64::from(clamp_or(gain_db, EqParam::Gain.range(), 0.0));
        let w0 = 2.0 * core::f64::consts::PI * freq / sr;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        // Amplitude: shelves and the peak use sqrt of the linear gain.
        let a = 10f64.powf(gain_db / 40.0);
        let sqrt_a2 = 2.0 * a.sqrt() * alpha;
        let (b0, b1, b2, a0, a1, a2) = match kind {
            EqKind::LowCut => (
                (1.0 + cos) / 2.0,
                -(1.0 + cos),
                (1.0 + cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            EqKind::HighCut => (
                (1.0 - cos) / 2.0,
                1.0 - cos,
                (1.0 - cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            EqKind::Peak => (
                1.0 + alpha * a,
                -2.0 * cos,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos,
                1.0 - alpha / a,
            ),
            EqKind::LowShelf => (
                a * ((a + 1.0) - (a - 1.0) * cos + sqrt_a2),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                a * ((a + 1.0) - (a - 1.0) * cos - sqrt_a2),
                (a + 1.0) + (a - 1.0) * cos + sqrt_a2,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                (a + 1.0) + (a - 1.0) * cos - sqrt_a2,
            ),
            EqKind::HighShelf => (
                a * ((a + 1.0) + (a - 1.0) * cos + sqrt_a2),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                a * ((a + 1.0) + (a - 1.0) * cos - sqrt_a2),
                (a + 1.0) - (a - 1.0) * cos + sqrt_a2,
                2.0 * ((a - 1.0) - (a + 1.0) * cos),
                (a + 1.0) - (a - 1.0) * cos - sqrt_a2,
            ),
        };
        Coeffs {
            b0: (b0 / a0) as f32,
            b1: (b1 / a0) as f32,
            b2: (b2 / a0) as f32,
            a1: (a1 / a0) as f32,
            a2: (a2 / a0) as f32,
        }
    }

    /// Magnitude response at `freq_hz`, in dB.
    #[must_use]
    pub fn response_db(&self, freq_hz: f32, sample_rate: f32) -> f32 {
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
            f64::from(sample_rate)
        } else {
            48_000.0
        };
        let f = f64::from(freq_hz.max(0.0)).min(sr * 0.5);
        let w = 2.0 * core::f64::consts::PI * f / sr;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let (b0, b1, b2) = (f64::from(self.b0), f64::from(self.b1), f64::from(self.b2));
        let (a1, a2) = (f64::from(self.a1), f64::from(self.a2));
        // H(e^jw) = (b0 + b1 e^-jw + b2 e^-2jw) / (1 + a1 e^-jw + a2 e^-2jw)
        let num_re = b0 + b1 * c1 + b2 * c2;
        let num_im = -(b1 * s1 + b2 * s2);
        let den_re = 1.0 + a1 * c1 + a2 * c2;
        let den_im = -(a1 * s1 + a2 * s2);
        let num = num_re * num_re + num_im * num_im;
        let den = (den_re * den_re + den_im * den_im).max(1e-30);
        (10.0 * (num / den).max(1e-30).log10()) as f32
    }
}

/// Transposed direct form II state of one biquad (two delay elements).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BiquadState {
    s1: f32,
    s2: f32,
}

impl BiquadState {
    /// Processes one sample through `c`.
    #[inline]
    pub fn tick(&mut self, x: f32, c: &Coeffs) -> f32 {
        let y = c.b0 * x + self.s1;
        self.s1 = flush_denormal(c.b1 * x - c.a1 * y + self.s2);
        self.s2 = flush_denormal(c.b2 * x - c.a2 * y);
        y
    }

    /// Clears the delay elements.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Runtime state of one band: its target values, the smoothed frequency / gain / wet amount
/// and the stereo biquad states.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BandDsp {
    band: EqBand,
    coeffs: Coeffs,
    freq: Smoother,
    gain: Smoother,
    /// `1.0` while the band is on, `0.0` while off, faded in between.
    wet: Smoother,
    left: BiquadState,
    right: BiquadState,
}

impl BandDsp {
    fn new(sample_rate: f32, band: EqBand) -> Self {
        let s = |v: f32| Smoother::new(sample_rate, EQ_SMOOTH_MS, v);
        let mut dsp = Self {
            band,
            coeffs: Coeffs::IDENTITY,
            freq: s(band.freq),
            gain: s(band.gain_db),
            wet: s(f32::from(u8::from(band.on))),
            left: BiquadState::default(),
            right: BiquadState::default(),
        };
        dsp.recompute(sample_rate, band.freq, band.gain_db);
        dsp
    }

    fn recompute(&mut self, sample_rate: f32, freq: f32, gain_db: f32) {
        self.coeffs = Coeffs::compute(self.band.kind, freq, gain_db, self.band.q, sample_rate);
    }

    /// Points the smoothers at the band's values; kind / Q / on changes apply at once.
    fn retarget(&mut self, sample_rate: f32, band: EqBand) {
        let kind_or_q_changed = band.kind != self.band.kind || band.q != self.band.q;
        self.band = band;
        self.freq.set_target(band.freq);
        self.gain.set_target(band.gain_db);
        self.wet.set_target(f32::from(u8::from(band.on)));
        if kind_or_q_changed {
            let (f, g) = (self.freq.current(), self.gain.current());
            self.recompute(sample_rate, f, g);
        }
    }

    /// Jumps straight to the band's values (project load / reset).
    fn snap(&mut self, sample_rate: f32, band: EqBand) {
        self.band = band;
        self.freq.snap(band.freq);
        self.gain.snap(band.gain_db);
        self.wet.snap(f32::from(u8::from(band.on)));
        self.recompute(sample_rate, band.freq, band.gain_db);
        self.left.reset();
        self.right.reset();
    }

    /// `true` while the band contributes anything (on, or still fading out).
    #[inline]
    fn active(&self) -> bool {
        self.band.on || self.wet.current() > 0.0
    }

    /// One stereo frame.
    #[inline]
    fn process(&mut self, sample_rate: f32, l: f32, r: f32) -> (f32, f32) {
        let moving = !self.freq.is_settled() || !self.gain.is_settled();
        let f = self.freq.tick();
        let g = self.gain.tick();
        if moving {
            self.recompute(sample_rate, f, g);
        }
        let wet = self.wet.tick();
        let (yl, yr) = (
            self.left.tick(l, &self.coeffs),
            self.right.tick(r, &self.coeffs),
        );
        if wet >= 1.0 {
            (yl, yr)
        } else {
            (l + (yl - l) * wet, r + (yr - r) * wet)
        }
    }
}

/// The seven-band EQ DSP.
#[derive(Debug, Clone, PartialEq)]
pub struct ParametricEq {
    sample_rate: f32,
    state: EqState,
    bands: [BandDsp; BAND_COUNT],
    /// `1.0` while the EQ is on, `0.0` while off, faded in between.
    on_mix: Smoother,
    /// Output gain, linear, smoothed.
    out_gain: Smoother,
}

impl ParametricEq {
    /// A flat EQ for `sample_rate` (non-finite or non-positive rates fall back to 48 kHz).
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let state = EqState::FLAT;
        Self {
            sample_rate,
            state,
            bands: state.bands.map(|b| BandDsp::new(sample_rate, b)),
            on_mix: Smoother::new(sample_rate, EQ_SMOOTH_MS, 1.0),
            out_gain: Smoother::new(sample_rate, EQ_SMOOTH_MS, 1.0),
        }
    }

    /// Sample rate the EQ runs at.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// The target state (what the UI mirrors).
    #[must_use]
    pub fn state(&self) -> &EqState {
        &self.state
    }

    /// Sets one band value (clamped); the change glides over [`EQ_SMOOTH_MS`]. Returns the
    /// stored value.
    pub fn set(&mut self, band: usize, param: EqParam, value: f32) -> f32 {
        let index = band % BAND_COUNT;
        let stored = self.state.bands[index].set(param, value);
        let sr = self.sample_rate;
        self.bands[index].retarget(sr, self.state.bands[index]);
        stored
    }

    /// Switches the whole EQ on or off (faded).
    pub fn set_on(&mut self, on: bool) {
        self.state.on = on;
        self.on_mix.set_target(f32::from(u8::from(on)));
    }

    /// Sets the output gain in dB (clamped, smoothed). Returns the stored value.
    pub fn set_gain_db(&mut self, gain_db: f32) -> f32 {
        let stored = self.state.set_gain_db(gain_db);
        self.out_gain.set_target(db_to_gain(stored));
        stored
    }

    /// Replaces the whole state at once, snapping every smoother and clearing the filter
    /// memories (project load). Values are normalised into range.
    pub fn set_state(&mut self, state: &EqState) {
        let mut state = *state;
        state.normalise();
        self.state = state;
        let sr = self.sample_rate;
        for (dsp, band) in self.bands.iter_mut().zip(&state.bands) {
            dsp.snap(sr, *band);
        }
        self.on_mix.snap(f32::from(u8::from(state.on)));
        self.out_gain.snap(db_to_gain(state.gain_db));
    }

    /// Clears the filter memories and snaps the smoothers to their targets.
    pub fn reset(&mut self) {
        let state = self.state;
        self.set_state(&state);
    }

    /// Processes one stereo frame. Non-finite inputs are treated as `0.0`; the output is
    /// always finite. Exactly transparent while off (once the fade has ended) and for a flat
    /// state.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let l = if l.is_finite() { l } else { 0.0 };
        let r = if r.is_finite() { r } else { 0.0 };
        let mix = self.on_mix.tick();
        let gain = self.out_gain.tick();
        if mix <= 0.0 {
            // Bypassed: keep the filter memories clear so switching back on starts clean.
            for b in &mut self.bands {
                b.left.reset();
                b.right.reset();
            }
            return (l, r);
        }
        let (mut yl, mut yr) = (l, r);
        let sr = self.sample_rate;
        for b in &mut self.bands {
            if b.active() {
                (yl, yr) = b.process(sr, yl, yr);
            }
        }
        yl *= gain;
        yr *= gain;
        if mix >= 1.0 {
            (yl, yr)
        } else {
            (l + (yl - l) * mix, r + (yr - r) * mix)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn kinds_and_params_round_trip() {
        for (i, k) in EqKind::ALL.iter().enumerate() {
            assert_eq!(k.index(), i);
            assert_eq!(EqKind::from_index(i), *k);
            assert_eq!(EqKind::from_value(i as f32 + 0.3), *k);
            assert!(!k.label().is_empty());
        }
        assert_eq!(EqKind::from_index(99), EqKind::HighCut);
        assert_eq!(EqKind::from_value(f32::NAN), EqKind::Peak);
        assert!(EqKind::Peak.has_gain() && !EqKind::LowCut.has_gain());
        let mut b = EqBand::default();
        assert_eq!(b.set(EqParam::Freq, 1.0), MIN_FREQ);
        assert_eq!(b.set(EqParam::Gain, 99.0), MAX_GAIN_DB);
        assert_eq!(b.set(EqParam::Q, f32::NAN), DEFAULT_Q);
        assert_eq!(b.set(EqParam::On, 0.0), 0.0);
        assert!(!b.on);
        assert_eq!(b.set(EqParam::Kind, 4.0), 4.0);
        assert_eq!(b.kind, EqKind::HighCut);
        for p in EqParam::ALL {
            assert!(!p.label().is_empty());
            assert!(p.range().start() <= p.range().end());
        }
    }

    #[test]
    fn default_is_the_flat_classic_layout() {
        let s = EqState::default();
        assert!(s.on && s.gain_db == 0.0 && s.is_flat());
        for (i, b) in s.bands.iter().enumerate() {
            assert_eq!(b.freq, EqState::LAYOUT_FREQS[i]);
            assert_eq!(b.kind, EqState::LAYOUT_KINDS[i]);
        }
        assert!(!s.bands[0].on && !s.bands[6].on);
        assert_eq!(preset_index_of(&s), Some(0));
        assert!(PRESETS.len() >= 12);
        let mut names: Vec<_> = PRESETS.iter().map(|p| p.name).collect();
        names.dedup();
        assert_eq!(names.len(), PRESETS.len(), "preset names are unique");
        for p in PRESETS {
            let mut n = p.state;
            n.normalise();
            assert_eq!(n, p.state, "{} is within range", p.name);
        }
    }

    #[test]
    fn identity_coefficients_and_responses() {
        let c = Coeffs::compute(EqKind::Peak, 1000.0, 0.0, 1.0, SR);
        assert_eq!(c.b0, 1.0);
        assert_eq!(c.b1, c.a1);
        assert_eq!(c.b2, c.a2);
        assert!(c.response_db(440.0, SR).abs() < 1e-6);
        let boost = Coeffs::compute(EqKind::Peak, 1000.0, 12.0, 1.0, SR);
        assert!((boost.response_db(1000.0, SR) - 12.0).abs() < 0.05);
        assert!(boost.response_db(100.0, SR).abs() < 0.3);
        let cut = Coeffs::compute(EqKind::LowCut, 200.0, 0.0, DEFAULT_Q, SR);
        assert!(cut.response_db(50.0, SR) < -20.0);
        assert!(cut.response_db(2000.0, SR).abs() < 0.2);
        let shelf = Coeffs::compute(EqKind::HighShelf, 10_000.0, 6.0, DEFAULT_Q, SR);
        assert!((shelf.response_db(20_000.0, SR) - 6.0).abs() < 0.6);
        assert!(shelf.response_db(100.0, SR).abs() < 0.1);
    }

    #[test]
    fn flat_and_off_are_transparent() {
        let mut eq = ParametricEq::new(SR);
        for i in 0..4000 {
            let x = ((i as f32) * 0.05).sin() * 0.7;
            assert_eq!(eq.process(x, -x), (x, -x), "flat at {i}");
        }
        eq.set(3, EqParam::Gain, 9.0);
        eq.set_on(false);
        eq.reset();
        for i in 0..4000 {
            let x = ((i as f32) * 0.05).sin() * 0.7;
            assert_eq!(eq.process(x, -x), (x, -x), "off at {i}");
        }
        let (l, r) = eq.process(f32::NAN, f32::INFINITY);
        assert_eq!((l, r), (0.0, 0.0));
    }

    #[test]
    fn state_response_sums_bands_and_gain() {
        let mut s = EqState::FLAT;
        s.set(3, EqParam::Gain, 6.0);
        s.set(5, EqParam::Gain, 3.0);
        s.set_gain_db(-2.0);
        let r = s.response_db(1000.0, SR);
        assert!((r - 4.0).abs() < 0.2, "{r}");
        s.on = false;
        assert_eq!(s.response_db(1000.0, SR), 0.0);
    }
}
