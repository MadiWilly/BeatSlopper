//! The per-voice contract and the click-free retrigger wrapper shared by all drums.

/// One procedural drum voice.
///
/// Voices are plain `Copy` state machines: [`trigger`](Self::trigger) restarts them,
/// [`tick`](Self::tick) produces one mono sample (exactly `0.0` once idle) and never allocates,
/// locks or panics.
pub trait Voice: Copy {
    /// Starts the hit. `velocity` is `0.0..=1.0` (linear amplitude), `tune` is the frequency
    /// ratio `2^(semitones / 12)` applied to tonal parts and noise-filter centres.
    fn trigger(&mut self, velocity: f32, tune: f32);
    /// Next mono sample.
    fn tick(&mut self) -> f32;
    /// `true` while any envelope of the voice is above the floor.
    fn is_active(&self) -> bool;
    /// Silences the voice immediately.
    fn reset(&mut self);
    /// Starts a fast fade to silence (used to choke the open hat). Default: no-op.
    fn choke(&mut self) {}
}

/// Length of the retrigger crossfade in seconds.
pub const RETRIGGER_FADE_SECONDS: f32 = 0.002;

/// Wraps a voice so that retriggering it while it sounds produces no click.
///
/// On a retrigger the sounding voice state is copied into a *tail* that fades out linearly
/// over [`RETRIGGER_FADE_SECONDS`] while the main voice restarts from scratch; the sum stays
/// continuous. Everything is `Copy`, so the copy is a handful of bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Retrig<V: Voice> {
    main: V,
    tail: V,
    tail_gain: f32,
    fade_step: f32,
}

impl<V: Voice> Retrig<V> {
    /// Wraps `voice` for `sample_rate`.
    #[must_use]
    pub fn new(sample_rate: f32, voice: V) -> Self {
        let fade_samples = if sample_rate.is_finite() && sample_rate > 0.0 {
            (sample_rate * RETRIGGER_FADE_SECONDS).max(1.0)
        } else {
            96.0
        };
        Self {
            main: voice,
            tail: voice,
            tail_gain: 0.0,
            fade_step: 1.0 / fade_samples,
        }
    }

    /// Fires the voice; a sounding voice is moved to the fading tail first.
    pub fn trigger(&mut self, velocity: f32, tune: f32) {
        if self.main.is_active() {
            self.tail = self.main;
            self.tail_gain = 1.0;
        }
        self.main.reset();
        self.main.trigger(velocity, tune);
    }

    /// Next mono sample (main voice plus fading tail).
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let mut out = if self.main.is_active() {
            self.main.tick()
        } else {
            0.0
        };
        if self.tail_gain > 0.0 {
            out += self.tail.tick() * self.tail_gain;
            self.tail_gain -= self.fade_step;
            if self.tail_gain <= 0.0 || !self.tail.is_active() {
                self.tail_gain = 0.0;
                self.tail.reset();
            }
        }
        out
    }

    /// `true` while the main voice or the tail is still sounding.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.main.is_active() || self.tail_gain > 0.0
    }

    /// Silences main voice and tail immediately.
    pub fn reset(&mut self) {
        self.main.reset();
        self.tail.reset();
        self.tail_gain = 0.0;
    }

    /// Chokes main voice and tail.
    pub fn choke(&mut self) {
        self.main.choke();
        self.tail.choke();
    }
}
