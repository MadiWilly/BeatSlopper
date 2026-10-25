//! Effects. The M2 chain is drive -> delay -> reverb ([`FxChain`]) followed by the engine's
//! master EQ ([`ParametricEq`], plugins milestone), master gain and [`Limiter`]; M8 adds the
//! synth channel insert drive -> lo-fi -> chorus -> sidechain ducker, which since M10 is the
//! per-track [`SynthInsert`](crate::track::SynthInsert).
//!
//! Every effect exposes a per-sample `process` that is allocation-free and panic-free once
//! constructed, treats non-finite input as silence, and flushes denormals on its feedback
//! paths (see [`crate::dsp`]).
//!
//! - [`drive`]: `tanh` waveshaper with gain compensation.
//! - [`delay`]: stereo delay line with feedback, tone and click-free time changes.
//! - [`reverb`]: Freeverb (8 combs + 4 allpasses per channel).
//! - [`lofi`]: bit crusher, rate reducer, wobble, vinyl noise and high cut (M8).
//! - [`chorus`]: two modulated taps, mono in, stereo out (M8).
//! - [`ducker`]: kick-triggered sidechain gain dip (M8).
//! - [`mod_delay`]: the fractional-read delay line behind the wobble and the chorus.
//! - [`chain`]: everything wired together, driven by [`crate::ParamId`] values and synced to
//!   the BPM.
//! - [`reverb`]: Freeverb (8 combs + 4 allpasses per channel) with pre-delay, width and wet
//!   filters ([`ReverbState`] / [`ReverbParam`]).
//! - [`eq`]: the seven-band master parametric EQ ([`EqState`] / [`EqBand`] / [`EqParam`]).
//! - [`chain`]: the three in series, driven by [`crate::ParamId`] values and synced to the BPM.
//! - [`limiter`]: the master soft clipper (M0).
//! - [`compressor`]: the feed-forward peak compressor of the M11 insert slots.
//! - [`eq3`]: the three-band insert EQ of the M11 inserts ([`Eq3State`] / [`Eq3`]).

pub mod chain;
pub mod chorus;
pub mod compressor;
pub mod delay;
pub mod drive;
pub mod ducker;
pub mod eq;
pub mod eq3;
pub mod limiter;
pub mod lofi;
pub mod mod_delay;
pub mod reverb;

pub use chain::{FxChain, MAX_DELAY_SECONDS, division_seconds};
pub use chorus::Chorus;
pub use compressor::Compressor;
pub use delay::StereoDelay;
pub use drive::Drive;
pub use ducker::Ducker;
pub use eq::{BAND_COUNT, EqBand, EqKind, EqParam, EqPreset, EqState, ParametricEq};
pub use eq3::{Eq3, Eq3State};
pub use limiter::Limiter;
pub use lofi::Lofi;
pub use mod_delay::ModDelay;
pub use reverb::{Reverb, ReverbParam, ReverbPreset, ReverbState};
