//! Engine core: message queue, polyphonic voices, test tone, smoothed master gain and the
//! master limiter.
//!
//! The public API below is the contract the `app` crate builds against (docs/ARCHITECTURE.md).
//!
//! Signal flow (M2 synth): `voices (sum) + TestTone -> master gain (smoothed) -> Limiter
//! (bypassable via `LimiterOn`) -> interleaved stereo`. The FX chain slots in between the
//! voices and the master gain once it is integrated.
//!
//! Real-time rules: everything used by [`Engine::render`] is pre-allocated in [`new_engine`].
//! `render` never allocates, locks, does I/O, logs or panics; it tolerates `out` buffers that
//! are shorter than `frames * 2` by rendering what fits.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use rtrb::{Consumer, Producer, RingBuffer};

use crate::drums::DrumRow;
use crate::dsp::db_to_gain;
use crate::fx::{FxChain, Limiter};
use crate::params::{DEFAULT_SMOOTH_MS, ParamId, Smoother};
use crate::pattern::Pattern;
use crate::synth::SineOsc;
use crate::transport::Transport;
use crate::voice::{SynthParams, VoicePool};

/// Capacity of the UI -> audio message queue.
pub const QUEUE_CAPACITY: usize = 1024;

/// Frequency of the M0 test tone (A4).
pub const TEST_TONE_HZ: f32 = 440.0;
/// Level of the test tone in dBFS (before master gain).
pub const TEST_TONE_DB: f32 = -12.0;
/// Fade in/out time of the test tone when toggled, in milliseconds.
pub const TEST_TONE_FADE_MS: f32 = 5.0;
/// Smoothing time constant of the master gain, in milliseconds.
pub const MASTER_GAIN_SMOOTH_MS: f32 = DEFAULT_SMOOTH_MS;
/// Largest master gain accepted by [`Engine::set_master_gain`] (+12 dB).
pub const MAX_MASTER_GAIN: f32 = 4.0;

/// Messages from the UI thread to the audio thread. Must stay `Copy`-cheap and allocation-free.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Message {
    /// Turn the M0 test tone (A4, 440 Hz) on or off.
    TestTone(bool),
    /// Master output gain, linear (1.0 = unity). Clamped to `0.0..=`[`MAX_MASTER_GAIN`].
    MasterGain(f32),
    /// Start a synth voice for MIDI `note` (69 = A4) at `velocity` in `0.0..=1.0`.
    /// A note that is already sounding retriggers its own voice.
    NoteOn {
        /// MIDI note number, `0..=127`.
        note: u8,
        /// Velocity, `0.0..=1.0`; scales the voice amplitude linearly.
        velocity: f32,
    },
    /// Release every voice playing MIDI `note` (envelope enters its release stage).
    NoteOff {
        /// MIDI note number, `0..=127`.
        note: u8,
    },
    /// Release every sounding voice (panic button / focus lost).
    AllNotesOff,
    /// Set a parameter; the value is clamped to [`ParamId::range`].
    SetParam(ParamId, f32),
    /// Tempo in beats per minute (60..=200); used by the BPM-synced delay and the M3 transport.
    SetBpm(f32),
    /// Start the sequencer from step 0.
    Play,
    /// Stop the sequencer, release its notes and rewind to step 0.
    Stop,
    /// Swing amount `0.0..=0.75`: odd steps are delayed by that fraction of a step.
    SetSwing(f32),
    /// Set one drum step.
    SetDrumStep {
        /// Drum row.
        row: DrumRow,
        /// Step index `0..16`.
        step: u8,
        /// Whether the step fires.
        on: bool,
        /// Accented (full velocity).
        accent: bool,
    },
    /// Set one synth-lane step.
    SetSynthStep {
        /// Step index `0..16`.
        step: u8,
        /// MIDI note, or `None` for a rest.
        note: Option<u8>,
        /// Accented (full velocity).
        accent: bool,
    },
    /// Clear one drum row.
    ClearRow(DrumRow),
    /// Clear the synth lane.
    ClearSynth,
    /// Clear every row and the synth lane.
    ClearAll,
    /// Per-row drum volume in dB (-24..=6).
    SetDrumVolume {
        /// Drum row.
        row: DrumRow,
        /// Volume in dB.
        db: f32,
    },
    /// Per-row drum tune in semitones (-12..=12).
    SetDrumTune {
        /// Drum row.
        row: DrumRow,
        /// Tune in semitones.
        semitones: f32,
    },
}

/// Where the sequencer is, as last published by the audio thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    /// Current step `0..16`.
    pub step: u8,
    /// Fraction of the current step already elapsed, `0.0..1.0`.
    pub frac: f32,
    /// Whether the transport is running.
    pub playing: bool,
}

/// Lock-free playhead state shared between [`Engine`] and [`EngineHandle`].
#[derive(Debug, Default)]
pub(crate) struct Shared {
    step: AtomicU32,
    /// Fraction of the step in thousandths.
    frac_milli: AtomicU32,
    playing: AtomicBool,
}

impl Shared {
    pub(crate) fn publish(&self, step: u8, frac: f32, playing: bool) {
        self.step.store(u32::from(step), Ordering::Relaxed);
        let milli = (frac.clamp(0.0, 0.999) * 1000.0) as u32;
        self.frac_milli.store(milli, Ordering::Relaxed);
        self.playing.store(playing, Ordering::Relaxed);
    }

    pub(crate) fn read(&self) -> Position {
        Position {
            step: (self.step.load(Ordering::Relaxed) % 16) as u8,
            frac: self.frac_milli.load(Ordering::Relaxed) as f32 / 1000.0,
            playing: self.playing.load(Ordering::Relaxed),
        }
    }
}

/// UI-side handle: sends [`Message`]s to the engine over a lock-free SPSC queue.
pub struct EngineHandle {
    producer: Producer<Message>,
    shared: Arc<Shared>,
}

impl EngineHandle {
    /// Sends a message. Returns `false` if the queue is full (the message is dropped).
    pub fn send(&mut self, msg: Message) -> bool {
        self.producer.push(msg).is_ok()
    }

    /// The sequencer playhead as last published by the audio thread (lock-free).
    #[must_use]
    pub fn position(&self) -> Position {
        self.shared.read()
    }

    /// Number of messages that can still be queued before [`send`](Self::send) starts failing.
    #[must_use]
    pub fn free_slots(&self) -> usize {
        self.producer.slots()
    }

    /// `true` once the audio side has dropped its [`Engine`] (e.g. the stream was closed).
    #[must_use]
    pub fn is_disconnected(&self) -> bool {
        self.producer.is_abandoned()
    }
}

/// The M0 test tone: a 440 Hz sine at -12 dBFS with a linear fade when toggled.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TestTone {
    osc: SineOsc,
    amplitude: f32,
    on: bool,
    /// Fade gain `0.0..=1.0`; ramps towards `1.0` while `on`, `0.0` otherwise.
    gate: f32,
    /// Per-sample change of `gate` (1 / fade length in samples).
    gate_step: f32,
}

impl TestTone {
    fn new(sample_rate: f32) -> Self {
        let fade_samples = (sample_rate * TEST_TONE_FADE_MS * 0.001).max(1.0);
        Self {
            osc: SineOsc::new(TEST_TONE_HZ, sample_rate),
            amplitude: db_to_gain(TEST_TONE_DB),
            on: false,
            gate: 0.0,
            gate_step: 1.0 / fade_samples,
        }
    }

    /// `true` while the tone is on or still fading out.
    fn is_active(&self) -> bool {
        self.on || self.gate > 0.0
    }

    #[inline]
    fn tick(&mut self) -> f32 {
        if self.on {
            if self.gate < 1.0 {
                self.gate = (self.gate + self.gate_step).min(1.0);
            }
        } else if self.gate > 0.0 {
            self.gate = (self.gate - self.gate_step).max(0.0);
        }
        if self.gate <= 0.0 {
            // Fully faded out: restart from phase 0 next time so the fade-in is identical.
            self.osc.reset();
            return 0.0;
        }
        self.osc.tick() * self.amplitude * self.gate
    }
}

/// Audio-side engine. Owns all DSP state. Lives on the audio thread (or in a test).
pub struct Engine {
    sample_rate: f32,
    consumer: Consumer<Message>,
    tone: TestTone,
    voices: VoicePool,
    /// Current value of every [`ParamId`] (clamped), indexed by [`ParamId::index`].
    params: [f32; ParamId::COUNT],
    master_gain: Smoother,
    /// Drive -> delay -> reverb, between the voices and the master stage.
    fx: FxChain,
    limiter: Limiter,
    /// `ParamId::LimiterOn`: when `false` the limiter stage is bypassed.
    limiter_on: bool,
    /// Tempo in BPM (60..=200), shared by the delay sync and the M3 transport.
    bpm: f32,
    /// Transport state (bpm/swing/playing) for the step scheduler.
    transport: Transport,
    /// The pattern the scheduler plays.
    pattern: Pattern,
    /// Playhead published to the UI.
    shared: Arc<Shared>,
}

/// Creates an engine and its UI-side handle.
///
/// All buffers and DSP state are allocated here, once; the returned [`Engine`] does not
/// allocate afterwards. `sample_rate` is in Hz (tests use 48 000).
pub fn new_engine(sample_rate: f32) -> (EngineHandle, Engine) {
    let (producer, consumer) = RingBuffer::new(QUEUE_CAPACITY);
    let shared = Arc::new(Shared::default());
    let mut params = [0.0_f32; ParamId::COUNT];
    for id in ParamId::ALL {
        params[id.index()] = id.default_value();
    }
    (
        EngineHandle {
            producer,
            shared: Arc::clone(&shared),
        },
        Engine {
            sample_rate,
            consumer,
            tone: TestTone::new(sample_rate),
            voices: VoicePool::new(sample_rate),
            params,
            master_gain: Smoother::new(
                sample_rate,
                MASTER_GAIN_SMOOTH_MS,
                ParamId::MasterGain.default_value(),
            ),
            fx: FxChain::new(sample_rate),
            limiter: Limiter::new(),
            limiter_on: ParamId::LimiterOn.default_value() >= 0.5,
            bpm: crate::transport::DEFAULT_BPM,
            transport: Transport::default(),
            pattern: Pattern::default(),
            shared,
        },
    )
}

impl Engine {
    /// Sample rate this engine was created for, in Hz.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Turns the test tone on or off. Takes effect with a [`TEST_TONE_FADE_MS`] fade.
    pub fn set_test_tone(&mut self, on: bool) {
        self.tone.on = on;
    }

    /// Whether the test tone is currently switched on (ignores the fade tail).
    #[must_use]
    pub fn test_tone(&self) -> bool {
        self.tone.on
    }

    /// Sets the master gain target (linear, clamped to `0.0..=`[`MAX_MASTER_GAIN`]).
    /// The change is smoothed over about [`MASTER_GAIN_SMOOTH_MS`]. Non-finite values are ignored.
    pub fn set_master_gain(&mut self, gain: f32) {
        if gain.is_finite() {
            let gain = gain.clamp(0.0, MAX_MASTER_GAIN);
            self.master_gain.set_target(gain);
            self.params[ParamId::MasterGain.index()] = gain;
        }
    }

    /// The master gain target most recently set (linear).
    #[must_use]
    pub fn master_gain(&self) -> f32 {
        self.master_gain.target()
    }

    /// Starts a synth voice for MIDI `note` (69 = A4 = 440 Hz) at `velocity` (`0.0..=1.0`,
    /// clamped). A note that is already sounding retriggers its own voice; when all
    /// [`MAX_VOICES`](crate::voice::MAX_VOICES) are busy the oldest releasing (else oldest
    /// held) voice is stolen.
    pub fn note_on(&mut self, note: u8, velocity: f32) {
        self.voices.note_on(note, velocity);
    }

    /// Releases every voice playing MIDI `note`; it fades out over the release time.
    pub fn note_off(&mut self, note: u8) {
        self.voices.note_off(note);
    }

    /// Releases every sounding voice.
    pub fn all_notes_off(&mut self) {
        self.voices.all_notes_off();
    }

    /// Sets parameter `id` to `value`, clamped to [`ParamId::range`] (non-finite values
    /// become the default). Synth parameters (oscillators, filter, envelope, LFO, glide) reach
    /// every voice immediately, running ones included; the master gain is smoothed as in
    /// [`set_master_gain`](Self::set_master_gain); `LimiterOn` switches the limiter stage.
    pub fn set_param(&mut self, id: ParamId, value: f32) {
        let value = id.clamp(value);
        self.params[id.index()] = value;
        match id {
            id if id.is_synth() => {
                self.voices.set_param(id, value);
            }
            ParamId::MasterGain => self.set_master_gain(value),
            ParamId::LimiterOn => self.limiter_on = value >= 0.5,
            id if id.is_fx() => self.fx.set_param(id, value),
            _ => {}
        }
    }

    /// The synth parameters currently applied to the voices.
    #[must_use]
    pub fn synth_params(&self) -> &SynthParams {
        self.voices.params()
    }

    /// The effect chain (read-only; parameters go through [`set_param`](Self::set_param)).
    #[must_use]
    pub fn fx(&self) -> &FxChain {
        &self.fx
    }

    /// Current BPM-synced delay time in samples.
    #[must_use]
    pub fn delay_samples(&self) -> usize {
        self.fx.delay_samples()
    }

    /// `true` while the master limiter stage is active (`ParamId::LimiterOn`).
    #[must_use]
    pub fn limiter_on(&self) -> bool {
        self.limiter_on
    }

    /// Sets the tempo (clamped to 60..=200 BPM). Used by the BPM-synced delay and the transport.
    pub fn set_bpm(&mut self, bpm: f32) {
        if bpm.is_finite() {
            self.bpm = bpm.clamp(60.0, 200.0);
            self.transport.set_bpm(self.bpm);
            self.fx.set_bpm(self.bpm);
        }
    }

    /// Current tempo in BPM.
    #[must_use]
    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    /// Starts the sequencer from step 0 (M3 stub: the scheduler lands with the sequencer work).
    pub fn play(&mut self) {
        self.transport.set_playing(true);
        self.shared.publish(0, 0.0, true);
    }

    /// Stops the sequencer, releases its notes and rewinds to step 0.
    pub fn stop(&mut self) {
        self.transport.set_playing(false);
        self.voices.all_notes_off();
        self.shared.publish(0, 0.0, false);
    }

    /// Sets the swing amount (`0.0..=0.75`).
    pub fn set_swing(&mut self, swing: f32) {
        self.transport.set_swing(swing);
    }

    /// The transport (bpm, swing, playing).
    #[must_use]
    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    /// The pattern the sequencer plays.
    #[must_use]
    pub fn pattern(&self) -> &Pattern {
        &self.pattern
    }

    /// Replaces the whole pattern (project load; call before the stream starts or accept a
    /// one-off copy on the audio thread).
    pub fn set_pattern(&mut self, pattern: &Pattern) {
        self.pattern = pattern.clone();
        for row in DrumRow::ALL {
            let i = row.index();
            self.set_drum_volume(row, self.pattern.drum_volume[i]);
            self.set_drum_tune(row, self.pattern.drum_tune[i]);
        }
    }

    /// Sets one drum step.
    pub fn set_drum_step(&mut self, row: DrumRow, step: u8, on: bool, accent: bool) {
        self.pattern.set_drum_step(row, step, on, accent);
    }

    /// Sets one synth-lane step.
    pub fn set_synth_step(&mut self, step: u8, note: Option<u8>, accent: bool) {
        self.pattern.set_synth_step(step, note, accent);
    }

    /// Per-row drum volume in dB.
    pub fn set_drum_volume(&mut self, row: DrumRow, db: f32) {
        self.pattern.set_drum_volume(row, db);
    }

    /// Per-row drum tune in semitones.
    pub fn set_drum_tune(&mut self, row: DrumRow, semitones: f32) {
        self.pattern.set_drum_tune(row, semitones);
    }

    /// Current (clamped) value of parameter `id`.
    #[must_use]
    pub fn param(&self, id: ParamId) -> f32 {
        self.params[id.index()]
    }

    /// The voice pool (for inspection: active voice count, per-voice state).
    #[must_use]
    pub fn voices(&self) -> &VoicePool {
        &self.voices
    }

    /// Number of synth voices currently sounding (held or releasing).
    #[must_use]
    pub fn active_voices(&self) -> usize {
        self.voices.active_voices()
    }

    /// `true` while the engine would produce non-silent output (any voice sounding, or the
    /// tone on or fading out).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.tone.is_active() || self.voices.is_active()
    }

    /// Applies all pending messages from the UI. Called at the start of every `render`.
    pub fn process_messages(&mut self) {
        while let Ok(msg) = self.consumer.pop() {
            match msg {
                Message::TestTone(on) => self.set_test_tone(on),
                Message::MasterGain(g) => self.set_master_gain(g),
                Message::NoteOn { note, velocity } => self.note_on(note, velocity),
                Message::NoteOff { note } => self.note_off(note),
                Message::AllNotesOff => self.all_notes_off(),
                Message::SetParam(id, value) => self.set_param(id, value),
                Message::SetBpm(bpm) => self.set_bpm(bpm),
                Message::Play => self.play(),
                Message::Stop => self.stop(),
                Message::SetSwing(s) => self.set_swing(s),
                Message::SetDrumStep {
                    row,
                    step,
                    on,
                    accent,
                } => self.pattern.set_drum_step(row, step, on, accent),
                Message::SetSynthStep { step, note, accent } => {
                    self.pattern.set_synth_step(step, note, accent)
                }
                Message::ClearRow(row) => self.pattern.clear_row(row),
                Message::ClearSynth => self.pattern.clear_synth(),
                Message::ClearAll => self.pattern.clear_all(),
                Message::SetDrumVolume { row, db } => self.set_drum_volume(row, db),
                Message::SetDrumTune { row, semitones } => self.set_drum_tune(row, semitones),
            }
        }
    }

    /// Renders `frames` frames of **interleaved stereo** into `out` (`out.len() >= frames * 2`).
    ///
    /// Real-time safe: no allocation, no locks, no I/O, no panics. If `out` is shorter than
    /// `frames * 2`, only the frames that fit are rendered; any samples of `out` beyond the
    /// rendered frames are zeroed.
    pub fn render(&mut self, out: &mut [f32], frames: usize) {
        self.process_messages();
        let frames = frames.min(out.len() / 2);
        let (body, tail) = out.split_at_mut(frames * 2);
        let (stereo_frames, _) = body.as_chunks_mut::<2>();
        let limiter_on = self.limiter_on;
        for [l, r] in stereo_frames {
            // Voices go through the effect chain; the diagnostic test tone bypasses it.
            let synth = self.voices.tick();
            let (fl, fr) = self.fx.process(synth, synth);
            let tone = self.tone.tick();
            let gain = self.master_gain.tick();
            let (pl, pr) = ((fl + tone) * gain, (fr + tone) * gain);
            if limiter_on {
                *l = self.limiter.process(pl);
                *r = self.limiter.process(pr);
            } else {
                *l = if pl.is_finite() { pl } else { 0.0 };
                *r = if pr.is_finite() { pr } else { 0.0 };
            }
        }
        tail.fill(0.0);
    }
}
