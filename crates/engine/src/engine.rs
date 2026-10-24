//! Engine core: message queue, test tone, smoothed master gain and the master limiter.
//!
//! The public API below is the contract the `app` crate builds against (docs/ARCHITECTURE.md).
//!
//! Signal flow (M0): `TestTone -> master gain (smoothed) -> Limiter -> interleaved stereo`.
//!
//! Real-time rules: everything used by [`Engine::render`] is pre-allocated in [`new_engine`].
//! `render` never allocates, locks, does I/O, logs or panics; it tolerates `out` buffers that
//! are shorter than `frames * 2` by rendering what fits.

use rtrb::{Consumer, Producer, RingBuffer};

use crate::dsp::db_to_gain;
use crate::fx::Limiter;
use crate::params::{DEFAULT_SMOOTH_MS, Smoother};
use crate::synth::SineOsc;

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
}

/// UI-side handle: sends [`Message`]s to the engine over a lock-free SPSC queue.
pub struct EngineHandle {
    producer: Producer<Message>,
}

impl EngineHandle {
    /// Sends a message. Returns `false` if the queue is full (the message is dropped).
    pub fn send(&mut self, msg: Message) -> bool {
        self.producer.push(msg).is_ok()
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
    master_gain: Smoother,
    limiter: Limiter,
}

/// Creates an engine and its UI-side handle.
///
/// All buffers and DSP state are allocated here, once; the returned [`Engine`] does not
/// allocate afterwards. `sample_rate` is in Hz (tests use 48 000).
pub fn new_engine(sample_rate: f32) -> (EngineHandle, Engine) {
    let (producer, consumer) = RingBuffer::new(QUEUE_CAPACITY);
    (
        EngineHandle { producer },
        Engine {
            sample_rate,
            consumer,
            tone: TestTone::new(sample_rate),
            master_gain: Smoother::new(sample_rate, MASTER_GAIN_SMOOTH_MS, 1.0),
            limiter: Limiter::new(),
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
            self.master_gain
                .set_target(gain.clamp(0.0, MAX_MASTER_GAIN));
        }
    }

    /// The master gain target most recently set (linear).
    #[must_use]
    pub fn master_gain(&self) -> f32 {
        self.master_gain.target()
    }

    /// `true` while the engine would produce non-silent output (tone on or fading out).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.tone.is_active()
    }

    /// Applies all pending messages from the UI. Called at the start of every `render`.
    pub fn process_messages(&mut self) {
        while let Ok(msg) = self.consumer.pop() {
            match msg {
                Message::TestTone(on) => self.set_test_tone(on),
                Message::MasterGain(g) => self.set_master_gain(g),
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
        for [l, r] in stereo_frames {
            let dry = self.tone.tick();
            let gain = self.master_gain.tick();
            let s = self.limiter.process(dry * gain);
            *l = s;
            *r = s;
        }
        tail.fill(0.0);
    }
}
