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
//! - [`voice`]: the polyphonic voice pool with voice stealing (M1) and the per-voice synth
//!   path (M2): two oscillators, filter, envelope, glide, typed [`SynthParams`].
//! - [`synth`]: band-limited [`synth::Oscillator`] / [`synth::Wave`], [`synth::Svf`] lowpass,
//!   [`synth::Lfo`], plus [`synth::SineOsc`] / [`synth::note_to_hz`] for the test tone.
//! - [`presets`]: built-in sounds as `(ParamId, value)` data ([`Preset`]).
//! - [`dsp`]: small numeric helpers (denormal flushing, dB conversion).
//! - [`fx`]: effects (master [`fx::Limiter`] and the M2 chain).
//! - [`pattern`]: patterns, lengths and the [`Bank`] (patterns A..H, song chain, play mode).
//! - [`drums`], [`transport`], [`project`], [`export`]: drum kit, transport, project file,
//!   offline WAV export.
//! - [`mixer`]: the channel strips (five drum rows plus eight tracks: level, pan, mute,
//!   solo, delay/reverb sends) and the pan law; [`engine::Meters`] carries the per-block
//!   peaks to the UI.
//! - [`track`]: the M10 instrument tracks ([`Track`], [`SynthInsert`], [`TrackState`]).
//! - [`spectrum`]: the lock-free master-output tap and FFT analyser behind the EQ plugin's
//!   display; `fx::eq` and `fx::reverb` hold the plugin states ([`EqState`], [`ReverbState`]).
//! - [`scope`]: the lock-free master-output tap ([`ScopeTap`] / [`ScopeReader`]) and the
//!   drum-hit bitmask for the UI visualizers.

#![warn(missing_docs)]

pub mod drums;
pub mod dsp;
pub mod engine;
pub mod export;
pub mod fx;
pub mod mixer;
pub mod params;
pub mod pattern;
pub mod presets;
pub mod project;
pub mod scope;
pub mod spectrum;
pub mod synth;
pub mod track;
pub mod transport;
pub mod voice;

pub use engine::{Engine, EngineHandle, Message, QUEUE_CAPACITY, new_engine};
pub use params::{Adsr, AdsrStage, ParamId, Smoother};
pub use presets::Preset;
pub use synth::{LfoTarget, Wave};
pub use voice::{MAX_VOICES, SynthParams, VoicePool};

pub use drums::DrumRow;
pub use engine::{Meters, MidiHandle, Position};
pub use export::{ExportError, ExportStats};
pub use mixer::{Channel, ChannelParam, MixerState, Strip};
pub use pattern::{
    BAR_STEPS, Bank, ChainEntry, DrumStep, MAX_CHAIN, MAX_NOTES, MAX_STEPS, Note, NoteTrack,
    PATTERN_COUNT, Pattern, PatternLength, PlayMode, STEPS, SynthStep,
};
pub use project::{Project, ProjectError};
pub use track::{MAX_TRACKS, SynthInsert, Track, TrackId, TrackState};

pub use fx::{EqBand, EqKind, EqParam, EqState, ReverbParam, ReverbState};
pub use scope::{SCOPE_CAPACITY_FRAMES, SCOPE_WINDOW_FRAMES, ScopeReader, ScopeTap, ScopeWindow};
pub use spectrum::{Analyzer, SPECTRUM_BINS, SPECTRUM_SIZE, SpectrumReader};
