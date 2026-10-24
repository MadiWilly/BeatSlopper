//! BeatSlopper engine: pure DSP and sequencing.
//!
//! This crate has **no UI and no audio-device dependencies**. Everything audible can be
//! rendered offline with [`Engine::render`], which is how the tests work.
//!
//! Real-time rules (docs/PLAN.md): [`Engine::render`] never allocates, locks, does I/O, logs or
//! panics. The UI talks to the engine only through the lock-free [`EngineHandle`] queue.
//!
//! Module map (see docs/ARCHITECTURE.md):
//! - [`engine`]: the audio-thread core (message queue, voices, test tone, master stage).
//! - [`params`]: parameter ids ([`ParamId`]), smoothing ([`Smoother`]) and the [`Adsr`]
//!   envelope.
//! - [`voice`]: the polyphonic voice pool with voice stealing (M1).
//! - [`dsp`]: small numeric helpers (denormal flushing, dB conversion).
//! - [`fx`]: effects; M0 ships only the master [`fx::Limiter`].
//! - [`synth`]: oscillators ([`synth::SineOsc`], [`synth::note_to_hz`]).
//! - [`drums`], [`transport`], [`project`]: skeletons for M3.

#![warn(missing_docs)]

pub mod drums;
pub mod dsp;
pub mod engine;
pub mod fx;
pub mod params;
pub mod project;
pub mod synth;
pub mod transport;
pub mod voice;

pub use engine::{Engine, EngineHandle, Message, QUEUE_CAPACITY, new_engine};
pub use params::{Adsr, AdsrStage, ParamId, Smoother};
pub use voice::{MAX_VOICES, VoicePool};
