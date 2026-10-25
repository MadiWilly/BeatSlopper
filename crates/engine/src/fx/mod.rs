//! Effects. The M2 chain is drive -> delay -> reverb ([`FxChain`]) followed by the engine's
//! master gain and [`Limiter`]; M8 adds the synth channel insert drive -> lo-fi -> chorus ->
//! sidechain ducker ([`FxChain::process_synth_insert`]).
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
//! - [`limiter`]: the master soft clipper (M0).

pub mod chain;
pub mod chorus;
pub mod delay;
pub mod drive;
pub mod ducker;
pub mod limiter;
pub mod lofi;
pub mod mod_delay;
pub mod reverb;

pub use chain::{FxChain, MAX_DELAY_SECONDS, division_seconds};
pub use chorus::Chorus;
pub use delay::StereoDelay;
pub use drive::Drive;
pub use ducker::Ducker;
pub use limiter::Limiter;
pub use lofi::Lofi;
pub use mod_delay::ModDelay;
pub use reverb::Reverb;
