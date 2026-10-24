//! Polyphonic voice allocation (M1): a fixed pool of voices with oldest-note stealing.
//!
//! Voices are pre-allocated in `new_engine`; the pool never grows on the audio thread.
//! Each [`Voice`] is a sine oscillator (M2 swaps in the PolyBLEP synth) shaped by an amplitude
//! [`Adsr`] and scaled by velocity. The [`VoicePool`] owns [`MAX_VOICES`] of them in a fixed
//! array and sums the sounding ones per sample.
//!
//! **Level:** one voice at velocity `1.0` peaks at [`VOICE_LEVEL_DB`] (-12 dBFS, same as the M0
//! test tone). A 16-voice chord therefore sums to at most about +12 dBFS before the master
//! gain, which the master limiter squashes softly instead of clipping.

use crate::dsp::db_to_gain;
use crate::params::{Adsr, ParamId};
use crate::synth::{SineOsc, note_to_hz};

/// Maximum number of simultaneously sounding synth voices.
pub const MAX_VOICES: usize = 16;

/// Peak level of a single voice at velocity `1.0`, in dBFS (before master gain).
pub const VOICE_LEVEL_DB: f32 = -12.0;

/// Highest valid MIDI note number.
pub const MAX_NOTE: u8 = 127;

/// Life cycle of one voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceState {
    /// Free; can be allocated without stealing.
    #[default]
    Idle,
    /// Key held; envelope in attack/decay/sustain.
    Held,
    /// Key released; envelope in release. Stolen before held voices.
    Releasing,
}

/// One synth voice: oscillator, note, velocity, amplitude envelope and an age stamp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Voice {
    osc: SineOsc,
    note: u8,
    velocity: f32,
    env: Adsr,
    /// Allocation counter value at the last `note_on`; smaller means older.
    age: u64,
}

impl Voice {
    /// Creates an idle voice for the given sample rate with the envelope `env`.
    #[must_use]
    pub fn new(sample_rate: f32, env: Adsr) -> Self {
        Self {
            osc: SineOsc::new(note_to_hz(69.0), sample_rate),
            note: 0,
            velocity: 0.0,
            env,
            age: 0,
        }
    }

    /// Current life-cycle state, derived from the envelope.
    #[must_use]
    pub fn state(&self) -> VoiceState {
        if self.env.is_idle() {
            VoiceState::Idle
        } else if self.env.is_releasing() {
            VoiceState::Releasing
        } else {
            VoiceState::Held
        }
    }

    /// MIDI note this voice plays (or last played).
    #[must_use]
    pub fn note(&self) -> u8 {
        self.note
    }

    /// Velocity in `0.0..=1.0` of the current note.
    #[must_use]
    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    /// Age stamp: the allocation counter at the last `note_on` (smaller = older).
    #[must_use]
    pub fn age(&self) -> u64 {
        self.age
    }

    /// The amplitude envelope.
    #[must_use]
    pub fn env(&self) -> &Adsr {
        &self.env
    }

    /// Mutable access to the amplitude envelope (for re-timing).
    pub fn env_mut(&mut self) -> &mut Adsr {
        &mut self.env
    }

    /// Starts `note` at `velocity`. A voice that is still sounding keeps its oscillator phase
    /// and envelope level (click-free retrigger); an idle one starts from phase zero.
    pub fn note_on(&mut self, note: u8, velocity: f32, sample_rate: f32, age: u64) {
        if self.env.is_idle() {
            self.osc.reset();
        }
        self.note = note;
        self.velocity = velocity;
        self.age = age;
        self.osc
            .set_frequency(note_to_hz(f32::from(note)), sample_rate);
        self.env.note_on();
    }

    /// Releases the voice (envelope enters its release stage).
    pub fn note_off(&mut self) {
        self.env.note_off();
    }

    /// Produces the next sample (already scaled by velocity and [`VOICE_LEVEL_DB`]).
    /// Returns exactly `0.0` for an idle voice without advancing anything.
    #[inline]
    pub fn tick(&mut self, voice_gain: f32) -> f32 {
        if self.env.is_idle() {
            return 0.0;
        }
        let env = self.env.tick();
        let s = self.osc.tick();
        s * env * self.velocity * voice_gain
    }
}

/// Fixed-size polyphonic voice pool with voice stealing.
///
/// Allocation order for `note_on`: the voice already playing that note (retrigger), else an
/// idle voice, else the oldest releasing voice, else the oldest held voice. Allocation-free;
/// all voices live in a fixed array.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoicePool {
    voices: [Voice; MAX_VOICES],
    sample_rate: f32,
    voice_gain: f32,
    /// Monotonic counter stamped onto voices at `note_on`.
    counter: u64,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
}

impl VoicePool {
    /// Creates a pool of idle voices with the default amplitude envelope.
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let attack = ParamId::AmpAttack.default_value();
        let decay = ParamId::AmpDecay.default_value();
        let sustain = ParamId::AmpSustain.default_value();
        let release = ParamId::AmpRelease.default_value();
        let env = Adsr::new(sample_rate, attack, decay, sustain, release);
        Self {
            voices: [Voice::new(sample_rate, env); MAX_VOICES],
            sample_rate,
            voice_gain: db_to_gain(VOICE_LEVEL_DB),
            counter: 0,
            attack,
            decay,
            sustain,
            release,
        }
    }

    /// Sample rate the pool renders at, in Hz.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// All voices (for inspection and tests).
    #[must_use]
    pub fn voices(&self) -> &[Voice; MAX_VOICES] {
        &self.voices
    }

    /// Number of voices that are not idle (held or releasing).
    #[must_use]
    pub fn active_voices(&self) -> usize {
        self.voices
            .iter()
            .filter(|v| v.state() != VoiceState::Idle)
            .count()
    }

    /// Number of voices whose key is held (attack/decay/sustain).
    #[must_use]
    pub fn held_voices(&self) -> usize {
        self.voices
            .iter()
            .filter(|v| v.state() == VoiceState::Held)
            .count()
    }

    /// `true` when at least one voice is sounding.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.voices.iter().any(|v| !v.env.is_idle())
    }

    /// Current amplitude envelope times `(attack, decay, sustain, release)`.
    #[must_use]
    pub fn adsr(&self) -> (f32, f32, f32, f32) {
        (self.attack, self.decay, self.sustain, self.release)
    }

    /// Sets the amplitude envelope for every voice, running ones included (their current level
    /// is preserved; only the remaining stages change speed). Values are clamped to the
    /// corresponding [`ParamId`] ranges.
    pub fn set_adsr(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.attack = ParamId::AmpAttack.clamp(attack);
        self.decay = ParamId::AmpDecay.clamp(decay);
        self.sustain = ParamId::AmpSustain.clamp(sustain);
        self.release = ParamId::AmpRelease.clamp(release);
        for v in &mut self.voices {
            v.env.set_times(
                self.sample_rate,
                self.attack,
                self.decay,
                self.sustain,
                self.release,
            );
        }
    }

    /// Index of the voice to use for `note`, following the stealing policy.
    fn allocate(&self, note: u8) -> usize {
        // 1. A voice already playing this note (held or releasing) retriggers.
        if let Some(i) = self
            .voices
            .iter()
            .position(|v| v.state() != VoiceState::Idle && v.note == note)
        {
            return i;
        }
        // 2. Any idle voice.
        if let Some(i) = self
            .voices
            .iter()
            .position(|v| v.state() == VoiceState::Idle)
        {
            return i;
        }
        // 3. The oldest releasing voice, else 4. the oldest held voice.
        let oldest = |state: VoiceState| {
            self.voices
                .iter()
                .enumerate()
                .filter(|(_, v)| v.state() == state)
                .min_by_key(|(_, v)| v.age)
                .map(|(i, _)| i)
        };
        oldest(VoiceState::Releasing)
            .or_else(|| oldest(VoiceState::Held))
            .unwrap_or(0)
    }

    /// Starts `note` (MIDI, clamped to `0..=127`) at `velocity` (clamped to `0.0..=1.0`;
    /// non-finite becomes `0.0`). Never fails: a voice is stolen when the pool is full.
    pub fn note_on(&mut self, note: u8, velocity: f32) {
        let note = note.min(MAX_NOTE);
        let velocity = if velocity.is_finite() {
            velocity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let i = self.allocate(note);
        self.counter = self.counter.wrapping_add(1);
        let age = self.counter;
        self.voices[i].note_on(note, velocity, self.sample_rate, age);
    }

    /// Releases every voice playing `note`.
    pub fn note_off(&mut self, note: u8) {
        let note = note.min(MAX_NOTE);
        for v in &mut self.voices {
            if v.state() == VoiceState::Held && v.note == note {
                v.note_off();
            }
        }
    }

    /// Releases every sounding voice (they fade out over their release time).
    pub fn all_notes_off(&mut self) {
        for v in &mut self.voices {
            v.note_off();
        }
    }

    /// Silences every voice instantly. Only for resets while the output is already silent.
    pub fn reset(&mut self) {
        for v in &mut self.voices {
            v.env.reset();
        }
    }

    /// Produces the next mono sample: the sum of all sounding voices.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let gain = self.voice_gain;
        self.voices.iter_mut().map(|v| v.tick(gain)).sum()
    }

    /// Adds the next `out.len()` mono samples onto `out` (mixing, not overwriting).
    pub fn render_add(&mut self, out: &mut [f32]) {
        for s in out {
            *s += self.tick();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn run(pool: &mut VoicePool, n: usize) {
        for _ in 0..n {
            pool.tick();
        }
    }

    #[test]
    fn idle_pool_is_silent() {
        let mut pool = VoicePool::new(SR);
        assert!(!pool.is_active());
        assert_eq!(pool.active_voices(), 0);
        for _ in 0..1000 {
            assert_eq!(pool.tick(), 0.0);
        }
    }

    #[test]
    fn note_on_allocates_distinct_voices_up_to_max() {
        let mut pool = VoicePool::new(SR);
        for n in 0..MAX_VOICES {
            pool.note_on(60 + n as u8, 1.0);
        }
        assert_eq!(pool.active_voices(), MAX_VOICES);
        assert_eq!(pool.held_voices(), MAX_VOICES);
        pool.note_on(100, 1.0);
        assert_eq!(pool.active_voices(), MAX_VOICES, "stealing keeps the count");
        // The oldest held voice (note 60) was stolen.
        assert!(!pool.voices().iter().any(|v| v.note() == 60));
        assert!(pool.voices().iter().any(|v| v.note() == 100));
    }

    #[test]
    fn same_note_retriggers_same_voice() {
        let mut pool = VoicePool::new(SR);
        pool.note_on(64, 1.0);
        run(&mut pool, 100);
        pool.note_on(64, 0.5);
        assert_eq!(pool.active_voices(), 1);
        assert_eq!(pool.voices()[0].velocity(), 0.5);
    }

    #[test]
    fn releasing_voices_are_stolen_before_held_ones() {
        let mut pool = VoicePool::new(SR);
        for n in 0..MAX_VOICES {
            pool.note_on(n as u8, 1.0);
        }
        // Release note 5 (an early one) and note 12; 5 is older.
        pool.note_off(12);
        pool.note_off(5);
        run(&mut pool, 10);
        pool.note_on(120, 1.0);
        let v120 = pool.voices().iter().find(|v| v.note() == 120).unwrap();
        assert_eq!(v120.state(), VoiceState::Held);
        assert!(
            !pool.voices().iter().any(|v| v.note() == 5),
            "oldest releasing stolen"
        );
        assert!(
            pool.voices().iter().any(|v| v.note() == 12),
            "newer releasing kept"
        );
    }

    #[test]
    fn note_off_and_all_notes_off_release_then_idle() {
        let mut pool = VoicePool::new(SR);
        pool.set_adsr(0.001, 0.01, 1.0, 0.05);
        pool.note_on(60, 1.0);
        pool.note_on(64, 1.0);
        run(&mut pool, 1000);
        pool.note_off(60);
        assert_eq!(pool.held_voices(), 1);
        assert_eq!(pool.active_voices(), 2);
        pool.all_notes_off();
        assert_eq!(pool.held_voices(), 0);
        run(&mut pool, (SR * 0.06) as usize);
        assert!(!pool.is_active());
    }

    #[test]
    fn velocity_and_level_scale_output() {
        let mut a = VoicePool::new(SR);
        let mut b = VoicePool::new(SR);
        a.set_adsr(0.001, 0.01, 1.0, 0.1);
        b.set_adsr(0.001, 0.01, 1.0, 0.1);
        a.note_on(69, 1.0);
        b.note_on(69, 0.5);
        run(&mut a, 2000);
        run(&mut b, 2000);
        let (mut pa, mut pb) = (0.0_f32, 0.0_f32);
        for _ in 0..2000 {
            pa = pa.max(a.tick().abs());
            pb = pb.max(b.tick().abs());
        }
        let expected = db_to_gain(VOICE_LEVEL_DB);
        assert!((pa - expected).abs() < expected * 0.02, "peak {pa}");
        assert!((pb - expected * 0.5).abs() < expected * 0.02, "peak {pb}");
    }

    #[test]
    fn weird_inputs_do_not_panic() {
        let mut pool = VoicePool::new(SR);
        pool.note_on(255, f32::NAN);
        pool.note_on(200, f32::INFINITY);
        pool.note_on(0, -3.0);
        pool.note_off(255);
        for _ in 0..1000 {
            assert!(pool.tick().is_finite());
        }
        let mut buf = [0.0_f32; 64];
        pool.render_add(&mut buf);
        assert!(buf.iter().all(|s| s.is_finite()));
    }
}
