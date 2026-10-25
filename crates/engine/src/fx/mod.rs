//! Effects. The M2 chain is drive -> delay -> reverb ([`FxChain`]) followed by the engine's
//! master EQ ([`ParametricEq`], plugins milestone), master gain and [`Limiter`].
//!
//! Every effect exposes a per-sample `process` that is allocation-free and panic-free once
//! constructed, treats non-finite input as silence, and flushes denormals on its feedback
//! paths (see [`crate::dsp`]).
//!
//! - [`drive`]: `tanh` waveshaper with gain compensation.
//! - [`delay`]: stereo delay line with feedback, tone and click-free time changes.
//! - [`reverb`]: Freeverb (8 combs + 4 allpasses per channel) with pre-delay, width and wet
//!   filters ([`ReverbState`] / [`ReverbParam`]).
//! - [`eq`]: the seven-band master parametric EQ ([`EqState`] / [`EqBand`] / [`EqParam`]).
//! - [`chain`]: the three in series, driven by [`crate::ParamId`] values and synced to the BPM.
//! - [`limiter`]: the master soft clipper (M0).

pub mod chain;
pub mod delay;
pub mod drive;
pub mod eq;
pub mod limiter;
pub mod reverb;

pub use chain::{FxChain, MAX_DELAY_SECONDS, division_seconds};
pub use delay::StereoDelay;
pub use drive::Drive;
pub use eq::{BAND_COUNT, EqBand, EqKind, EqParam, EqPreset, EqState, ParametricEq};
pub use limiter::Limiter;
pub use reverb::{Reverb, ReverbParam, ReverbPreset, ReverbState};
