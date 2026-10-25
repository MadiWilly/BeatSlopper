//! Polyphonic voice allocation and the per-voice synth signal path.
//!
//! Voices are pre-allocated in `new_engine`; the pool never grows on the audio thread.
//! The [`VoicePool`] owns [`MAX_VOICES`] voices in a fixed array, the shared [`Lfo`] and the
//! current [`SynthParams`], and sums the sounding voices per sample.
//!
//! **Per-voice signal flow (M8):**
//!
//! ```text
//! subtractive mode:
//!   pitch = note (glide) + pitch env (one-pole decay from PitchEnvAmount) + LFO vibrato
//!   osc 1 x unison (1..=7 copies, +-Osc1Spread cents, / sqrt(n)) <- FM (sine modulator at
//!       FmRatio x f1, index 0..=8 rad into the phase)  ... x Osc1Mix
//!   + osc 2 (own detune)                                       x Osc2Mix
//!   + sub (sine, one octave below osc 1)                       x SubLevel
//!   + noise (white | pink)                                     x NoiseLevel
//! pluck mode: a Karplus-Strong string ([`Pluck`]) replaces all of the above.
//!   -> SVF (LP | BP | HP; cutoff = base x 2^(filter env x FilterEnvAmount x 4 oct
//!                        + (note - 60) / 12 x FilterKeyTrack
//!                        + (vel - 1) x VelToCutoff x 2 oct + LFO) )
//!   -> x amp env x (1 - VelToAmp + VelToAmp x vel) x voice level
//! ```
//!
//! **Level:** one oscillator at mix `1.0` and velocity `1.0` peaks at [`VOICE_LEVEL_DB`]
//! (-12 dBFS, same as the M0 test tone) before the filter. The default patch mixes two
//! oscillators (0.7 + 0.45), so a 16-voice chord sums to roughly +13 dBFS before the master
//! gain, which the master limiter squashes softly instead of clipping.
//!
//! **Modulation rate:** oscillator frequencies are recomputed immediately when a note starts
//! or a parameter changes, and otherwise at most every [`MOD_INTERVAL`] samples while the
//! pitch moves (glide, pitch envelope, pitch LFO) and only when it moved by more than a tiny
//! epsilon; the filter coefficients are refreshed every [`MOD_INTERVAL`] samples (and skipped
//! when nothing changed). Sub, noise and FM cost nothing while their level/amount is zero.
//!
//! **Memory:** every voice owns a pre-allocated string delay line (about 1 500 samples at
//! 48 kHz, sized in [`Voice::new`]); nothing allocates after construction.

use crate::dsp::db_to_gain;
use crate::params::{Adsr, ParamId};
use crate::synth::{
    FilterType, FmModulator, Lfo, LfoTarget, Noise, NoiseColor, Oscillator, Pluck, Svf, Wave,
    cents_to_ratio, fm_index, note_to_hz,
};

/// Maximum number of simultaneously sounding synth voices.
pub const MAX_VOICES: usize = 16;

/// Peak level of a single oscillator at mix `1.0` and velocity `1.0`, in dBFS (before master
/// gain).
pub const VOICE_LEVEL_DB: f32 = -12.0;

/// Highest valid MIDI note number.
pub const MAX_NOTE: u8 = 127;

/// Samples between filter coefficient updates (envelope and LFO -> cutoff).
pub const MOD_INTERVAL: u8 = 8;

/// Octaves of cutoff sweep at filter envelope amount `1.0`.
pub const ENV_CUTOFF_OCTAVES: f32 = 4.0;
/// Octaves of cutoff sweep at LFO depth `1.0` when the LFO targets the cutoff.
pub const LFO_CUTOFF_OCTAVES: f32 = 2.0;
/// Semitones of pitch sweep at LFO depth `1.0` when the LFO targets the pitch.
pub const LFO_PITCH_SEMITONES: f32 = 1.0;

/// Octaves the cutoff drops between velocity `1.0` and `0.0` at `VelToCutoff` `1.0`.
pub const VEL_CUTOFF_OCTAVES: f32 = 2.0;
/// Most unison copies of oscillator 1 (`Osc1Unison` range end).
pub const MAX_UNISON: usize = 7;

/// The glide parameter is the time for the pitch to cover 95 % of the jump (three time
/// constants of the one-pole slew).
const GLIDE_TIME_CONSTANTS: f32 = 3.0;
/// The pitch envelope decay parameter is the time to cover 95 % of the sweep (three time
/// constants of the one-pole decay).
const PITCH_ENV_TIME_CONSTANTS: f32 = 3.0;
/// Pitch envelope magnitude (semitones) below which it snaps to zero.
const PITCH_ENV_FLOOR: f32 = 1.0e-3;
/// Pitch change (semitones) below which the oscillators are not retuned.
const PITCH_EPSILON: f32 = 1.0e-5;
/// Phase spacing (turns, golden ratio) between unison copies at note start, so a supersaw
/// does not begin as one coherent spike.
const UNISON_PHASE_STEP: f64 = 0.618_033_988_749_895;
/// `OscMode` index of the plucked-string mode.
const PLUCK_MODE: u8 = 1;

/// Every parameter the voice pool consumes, as typed values. `Copy` and allocation-free so
/// it can be built on the UI side and handed to the audio thread by value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SynthParams {
    /// Oscillator 1 waveform.
    pub osc1_wave: Wave,
    /// Oscillator 1 detune, cents (`-50..=50`).
    pub osc1_detune: f32,
    /// Oscillator 1 level, `0.0..=1.0`.
    pub osc1_mix: f32,
    /// Oscillator 2 waveform.
    pub osc2_wave: Wave,
    /// Oscillator 2 detune, cents (`-50..=50`).
    pub osc2_detune: f32,
    /// Oscillator 2 level, `0.0..=1.0`.
    pub osc2_mix: f32,
    /// Oscillator 1 pulse width, `0.05..=0.95` (M8; `0.5` = square).
    pub osc1_pulse: f32,
    /// Oscillator 2 pulse width, `0.05..=0.95` (M8).
    pub osc2_pulse: f32,
    /// Oscillator 1 unison voice count, `1..=7` (M8).
    pub osc1_unison: u8,
    /// Oscillator 1 unison spread, cents (`0.0..=50.0`, M8).
    pub osc1_spread: f32,
    /// Sub oscillator level, `0.0..=1.0` (M8).
    pub sub_level: f32,
    /// Noise level, `0.0..=1.0` (M8).
    pub noise_level: f32,
    /// Noise colour index: 0 white, 1 pink (M8).
    pub noise_color: u8,
    /// FM amount, `0.0..=1.0` (M8).
    pub fm_amount: f32,
    /// FM modulator ratio, `0.5..=8.0` (M8).
    pub fm_ratio: f32,
    /// Oscillator mode index: 0 subtractive, 1 pluck (M8).
    pub osc_mode: u8,
    /// Pluck damping, `0.0..=1.0` (M8).
    pub pluck_damping: f32,
    /// Pluck brightness, `0.0..=1.0` (M8).
    pub pluck_bright: f32,
    /// Base filter cutoff, Hz.
    pub cutoff: f32,
    /// Filter resonance, `0.0..=1.0`.
    pub resonance: f32,
    /// Filter envelope amount, `-1.0..=1.0` (+-[`ENV_CUTOFF_OCTAVES`] octaves).
    pub env_amount: f32,
    /// Filter type index: 0 lowpass, 1 bandpass, 2 highpass (M8).
    pub filter_type: u8,
    /// Filter envelope attack, seconds (M8).
    pub filter_attack: f32,
    /// Filter envelope decay, seconds (M8).
    pub filter_decay: f32,
    /// Filter envelope sustain level, `0.0..=1.0` (M8).
    pub filter_sustain: f32,
    /// Filter envelope release, seconds (M8).
    pub filter_release: f32,
    /// Filter cutoff key tracking, `0.0..=1.0` (M8).
    pub filter_key_track: f32,
    /// Amp attack, seconds.
    pub attack: f32,
    /// Amp decay, seconds.
    pub decay: f32,
    /// Amp sustain level, `0.0..=1.0`.
    pub sustain: f32,
    /// Amp release, seconds.
    pub release: f32,
    /// LFO rate, Hz.
    pub lfo_rate: f32,
    /// LFO depth, `0.0..=1.0`.
    pub lfo_depth: f32,
    /// What the LFO modulates.
    pub lfo_target: LfoTarget,
    /// Glide time, seconds (`0.0` = off).
    pub glide: f32,
    /// Pitch envelope amount, semitones (`-24.0..=24.0`, M8).
    pub pitch_env_amount: f32,
    /// Pitch envelope decay, seconds (M8).
    pub pitch_env_decay: f32,
    /// Velocity -> cutoff amount, `0.0..=1.0` (M8).
    pub vel_to_cutoff: f32,
    /// Velocity -> amplitude amount, `0.0..=1.0` (M8).
    pub vel_to_amp: f32,
}

/// Rounds a clamped index-style parameter value to its `u8` index.
#[inline]
fn index_u8(value: f32) -> u8 {
    // The value is already clamped into a small non-negative range by `ParamId::clamp`.
    value.round().clamp(0.0, 255.0) as u8
}

impl Default for SynthParams {
    fn default() -> Self {
        Self::from_fn(ParamId::default_value)
    }
}

impl SynthParams {
    /// Builds the struct by asking `value` for every synth [`ParamId`] (values are clamped).
    #[must_use]
    pub fn from_fn(mut value: impl FnMut(ParamId) -> f32) -> Self {
        let mut v = |id: ParamId| id.clamp(value(id));
        Self {
            osc1_wave: Wave::from_param(v(ParamId::Osc1Wave)),
            osc1_detune: v(ParamId::Osc1Detune),
            osc1_mix: v(ParamId::Osc1Mix),
            osc2_wave: Wave::from_param(v(ParamId::Osc2Wave)),
            osc2_detune: v(ParamId::Osc2Detune),
            osc2_mix: v(ParamId::Osc2Mix),
            osc1_pulse: v(ParamId::Osc1Pulse),
            osc2_pulse: v(ParamId::Osc2Pulse),
            osc1_unison: index_u8(v(ParamId::Osc1Unison)),
            osc1_spread: v(ParamId::Osc1Spread),
            sub_level: v(ParamId::SubLevel),
            noise_level: v(ParamId::NoiseLevel),
            noise_color: index_u8(v(ParamId::NoiseColor)),
            fm_amount: v(ParamId::FmAmount),
            fm_ratio: v(ParamId::FmRatio),
            osc_mode: index_u8(v(ParamId::OscMode)),
            pluck_damping: v(ParamId::PluckDamping),
            pluck_bright: v(ParamId::PluckBright),
            cutoff: v(ParamId::FilterCutoff),
            resonance: v(ParamId::FilterResonance),
            env_amount: v(ParamId::FilterEnvAmount),
            filter_type: index_u8(v(ParamId::FilterType)),
            filter_attack: v(ParamId::FilterAttack),
            filter_decay: v(ParamId::FilterDecay),
            filter_sustain: v(ParamId::FilterSustain),
            filter_release: v(ParamId::FilterRelease),
            filter_key_track: v(ParamId::FilterKeyTrack),
            attack: v(ParamId::AmpAttack),
            decay: v(ParamId::AmpDecay),
            sustain: v(ParamId::AmpSustain),
            release: v(ParamId::AmpRelease),
            lfo_rate: v(ParamId::LfoRate),
            lfo_depth: v(ParamId::LfoDepth),
            lfo_target: LfoTarget::from_param(v(ParamId::LfoTarget)),
            glide: v(ParamId::Glide),
            pitch_env_amount: v(ParamId::PitchEnvAmount),
            pitch_env_decay: v(ParamId::PitchEnvDecay),
            vel_to_cutoff: v(ParamId::VelToCutoff),
            vel_to_amp: v(ParamId::VelToAmp),
        }
    }

    /// Builds the struct from the engine's parameter array (indexed by [`ParamId::index`]).
    #[must_use]
    pub fn from_values(values: &[f32; ParamId::COUNT]) -> Self {
        Self::from_fn(|id| values[id.index()])
    }

    /// Sets one parameter (clamped to its range). Returns `false`, changing nothing, for ids
    /// that are not synth parameters.
    pub fn set(&mut self, id: ParamId, value: f32) -> bool {
        let value = id.clamp(value);
        match id {
            ParamId::Osc1Wave => self.osc1_wave = Wave::from_param(value),
            ParamId::Osc1Detune => self.osc1_detune = value,
            ParamId::Osc1Mix => self.osc1_mix = value,
            ParamId::Osc2Wave => self.osc2_wave = Wave::from_param(value),
            ParamId::Osc2Detune => self.osc2_detune = value,
            ParamId::Osc2Mix => self.osc2_mix = value,
            ParamId::Osc1Pulse => self.osc1_pulse = value,
            ParamId::Osc2Pulse => self.osc2_pulse = value,
            ParamId::Osc1Unison => self.osc1_unison = index_u8(value),
            ParamId::Osc1Spread => self.osc1_spread = value,
            ParamId::SubLevel => self.sub_level = value,
            ParamId::NoiseLevel => self.noise_level = value,
            ParamId::NoiseColor => self.noise_color = index_u8(value),
            ParamId::FmAmount => self.fm_amount = value,
            ParamId::FmRatio => self.fm_ratio = value,
            ParamId::OscMode => self.osc_mode = index_u8(value),
            ParamId::PluckDamping => self.pluck_damping = value,
            ParamId::PluckBright => self.pluck_bright = value,
            ParamId::FilterCutoff => self.cutoff = value,
            ParamId::FilterResonance => self.resonance = value,
            ParamId::FilterEnvAmount => self.env_amount = value,
            ParamId::FilterType => self.filter_type = index_u8(value),
            ParamId::FilterAttack => self.filter_attack = value,
            ParamId::FilterDecay => self.filter_decay = value,
            ParamId::FilterSustain => self.filter_sustain = value,
            ParamId::FilterRelease => self.filter_release = value,
            ParamId::FilterKeyTrack => self.filter_key_track = value,
            ParamId::AmpAttack => self.attack = value,
            ParamId::AmpDecay => self.decay = value,
            ParamId::AmpSustain => self.sustain = value,
            ParamId::AmpRelease => self.release = value,
            ParamId::LfoRate => self.lfo_rate = value,
            ParamId::LfoDepth => self.lfo_depth = value,
            ParamId::LfoTarget => self.lfo_target = LfoTarget::from_param(value),
            ParamId::Glide => self.glide = value,
            ParamId::PitchEnvAmount => self.pitch_env_amount = value,
            ParamId::PitchEnvDecay => self.pitch_env_decay = value,
            ParamId::VelToCutoff => self.vel_to_cutoff = value,
            ParamId::VelToAmp => self.vel_to_amp = value,
            _ => return false,
        }
        true
    }

    /// Reads one parameter as its `f32` parameter value; `None` for non-synth ids.
    #[must_use]
    pub fn get(&self, id: ParamId) -> Option<f32> {
        Some(match id {
            ParamId::Osc1Wave => self.osc1_wave.index() as f32,
            ParamId::Osc1Detune => self.osc1_detune,
            ParamId::Osc1Mix => self.osc1_mix,
            ParamId::Osc2Wave => self.osc2_wave.index() as f32,
            ParamId::Osc2Detune => self.osc2_detune,
            ParamId::Osc2Mix => self.osc2_mix,
            ParamId::Osc1Pulse => self.osc1_pulse,
            ParamId::Osc2Pulse => self.osc2_pulse,
            ParamId::Osc1Unison => f32::from(self.osc1_unison),
            ParamId::Osc1Spread => self.osc1_spread,
            ParamId::SubLevel => self.sub_level,
            ParamId::NoiseLevel => self.noise_level,
            ParamId::NoiseColor => f32::from(self.noise_color),
            ParamId::FmAmount => self.fm_amount,
            ParamId::FmRatio => self.fm_ratio,
            ParamId::OscMode => f32::from(self.osc_mode),
            ParamId::PluckDamping => self.pluck_damping,
            ParamId::PluckBright => self.pluck_bright,
            ParamId::FilterCutoff => self.cutoff,
            ParamId::FilterResonance => self.resonance,
            ParamId::FilterEnvAmount => self.env_amount,
            ParamId::FilterType => f32::from(self.filter_type),
            ParamId::FilterAttack => self.filter_attack,
            ParamId::FilterDecay => self.filter_decay,
            ParamId::FilterSustain => self.filter_sustain,
            ParamId::FilterRelease => self.filter_release,
            ParamId::FilterKeyTrack => self.filter_key_track,
            ParamId::AmpAttack => self.attack,
            ParamId::AmpDecay => self.decay,
            ParamId::AmpSustain => self.sustain,
            ParamId::AmpRelease => self.release,
            ParamId::LfoRate => self.lfo_rate,
            ParamId::LfoDepth => self.lfo_depth,
            ParamId::LfoTarget => self.lfo_target.index() as f32,
            ParamId::Glide => self.glide,
            ParamId::PitchEnvAmount => self.pitch_env_amount,
            ParamId::PitchEnvDecay => self.pitch_env_decay,
            ParamId::VelToCutoff => self.vel_to_cutoff,
            ParamId::VelToAmp => self.vel_to_amp,
            _ => return None,
        })
    }

    /// A copy with every field clamped to its [`ParamId`] range (non-finite -> default).
    #[must_use]
    pub fn clamped(&self) -> Self {
        Self::from_fn(|id| self.get(id).unwrap_or_else(|| id.default_value()))
    }

    /// Glide one-pole coefficient per sample (`1.0` = instant) for this glide time.
    fn glide_coeff(&self, sample_rate: f32) -> f32 {
        if self.glide <= 0.0 || !sample_rate.is_finite() || sample_rate <= 0.0 {
            return 1.0;
        }
        let tau_samples = sample_rate * self.glide / GLIDE_TIME_CONSTANTS;
        (1.0 - (-1.0 / tau_samples).exp()).clamp(0.0, 1.0)
    }

    /// Pitch envelope per-sample decay multiplier for this decay time.
    fn pitch_env_coeff(&self, sample_rate: f32) -> f32 {
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let decay = if self.pitch_env_decay.is_finite() {
            self.pitch_env_decay.max(1.0e-3)
        } else {
            ParamId::PitchEnvDecay.default_value()
        };
        (-PITCH_ENV_TIME_CONSTANTS / (sr * decay))
            .exp()
            .clamp(0.0, 1.0)
    }

    /// `true` when the plucked-string mode is selected.
    #[must_use]
    pub fn is_pluck_mode(&self) -> bool {
        self.osc_mode == PLUCK_MODE
    }
}

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

/// One synth voice: oscillator 1 with its unison copies, oscillator 2, sub, noise, FM
/// modulator and plucked string, the multimode filter, the amp and filter envelopes, the
/// pitch envelope, note, velocity, glide state and an age stamp.
///
/// Not `Copy`: the string's delay line is heap-allocated once in [`Voice::new`].
#[derive(Debug, Clone, PartialEq)]
pub struct Voice {
    osc1: [Oscillator; MAX_UNISON],
    osc2: Oscillator,
    sub: Oscillator,
    noise: Noise,
    fm: FmModulator,
    pluck: Pluck,
    filter: Svf,
    env: Adsr,
    filter_env: Adsr,
    note: u8,
    velocity: f32,
    /// Allocation counter value at the last `note_on`; smaller means older.
    age: u64,
    /// Current pitch in MIDI semitones (glides towards `pitch_target`). `f64` so the one-pole
    /// slew never stalls when `coeff * diff` drops below an `f32` ulp of the pitch.
    pitch: f64,
    pitch_target: f64,
    glide_coeff: f64,
    /// Detune ratios of the two oscillators.
    ratio1: f32,
    ratio2: f32,
    /// Detune ratio of each unison copy relative to oscillator 1, the number in use and the
    /// `1 / sqrt(n)` level compensation.
    unison_ratio: [f32; MAX_UNISON],
    unison_count: usize,
    unison_gain: f32,
    /// Pitch envelope: current offset (semitones), per-sample decay and start value.
    pitch_env: f32,
    pitch_env_coeff: f32,
    pitch_env_amount: f32,
    /// Semitone value the oscillators were last tuned to.
    tuned_semis: f32,
    /// Per-note cutoff offset in octaves (key tracking + velocity).
    note_octaves: f32,
    /// Per-note amplitude from velocity (`1 - VelToAmp + VelToAmp * vel`).
    vel_gain: f32,
    key_track: f32,
    vel_to_cutoff: f32,
    vel_to_amp: f32,
    pluck_mode: bool,
    /// A pluck is due as soon as the string is tuned to the new note.
    pluck_pending: bool,
    /// Set when the oscillator frequencies must be recomputed on the next tick.
    freq_dirty: bool,
    /// Countdown to the next filter coefficient update.
    mod_counter: u8,
}

impl Voice {
    /// Creates an idle voice for the given sample rate with the amp envelope `env` and the
    /// default synth parameters. Allocates the string's delay line (once).
    #[must_use]
    pub fn new(sample_rate: f32, env: Adsr) -> Self {
        Self::with_seed(sample_rate, env, 1)
    }

    /// Like [`new`](Self::new) with a noise seed, so voices in a pool do not share a noise
    /// sequence.
    #[must_use]
    pub fn with_seed(sample_rate: f32, env: Adsr, seed: u32) -> Self {
        let p = SynthParams::default();
        let hz = note_to_hz(69.0);
        let mut v = Self {
            osc1: [Oscillator::new(p.osc1_wave, hz, sample_rate); MAX_UNISON],
            osc2: Oscillator::new(p.osc2_wave, hz, sample_rate),
            sub: Oscillator::new(Wave::Sine, hz * 0.5, sample_rate),
            noise: Noise::new(seed),
            fm: FmModulator::new(sample_rate, hz),
            pluck: Pluck::new(sample_rate, seed.wrapping_mul(0x9E37_79B9)),
            filter: Svf::new(sample_rate, p.cutoff, p.resonance),
            env,
            filter_env: Adsr::new(
                sample_rate,
                p.filter_attack,
                p.filter_decay,
                p.filter_sustain,
                p.filter_release,
            ),
            note: 0,
            velocity: 0.0,
            age: 0,
            pitch: 69.0,
            pitch_target: 69.0,
            glide_coeff: 1.0,
            ratio1: 1.0,
            ratio2: 1.0,
            unison_ratio: [1.0; MAX_UNISON],
            unison_count: 1,
            unison_gain: 1.0,
            pitch_env: 0.0,
            pitch_env_coeff: 0.0,
            pitch_env_amount: 0.0,
            tuned_semis: f32::NAN,
            note_octaves: 0.0,
            vel_gain: 0.0,
            key_track: 0.0,
            vel_to_cutoff: 0.0,
            vel_to_amp: 1.0,
            pluck_mode: false,
            pluck_pending: false,
            freq_dirty: true,
            mod_counter: 0,
        };
        v.apply_params(&p, sample_rate);
        v
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

    /// Current (glided) pitch in MIDI semitones, before LFO modulation.
    #[must_use]
    pub fn pitch(&self) -> f32 {
        self.pitch as f32
    }

    /// The filter (for inspection: current cutoff after modulation, and its type).
    #[must_use]
    pub fn filter(&self) -> &Svf {
        &self.filter
    }

    /// The filter envelope.
    #[must_use]
    pub fn filter_env(&self) -> &Adsr {
        &self.filter_env
    }

    /// Current pitch envelope offset in semitones (decays towards `0.0`).
    #[must_use]
    pub fn pitch_env(&self) -> f32 {
        self.pitch_env
    }

    /// The plucked string (for inspection).
    #[must_use]
    pub fn pluck(&self) -> &Pluck {
        &self.pluck
    }

    /// `true` while the voice renders the plucked string instead of the oscillators.
    #[must_use]
    pub fn is_pluck_mode(&self) -> bool {
        self.pluck_mode
    }

    /// Number of unison copies of oscillator 1 in use.
    #[must_use]
    pub fn unison_count(&self) -> usize {
        self.unison_count
    }

    /// Applies new synth parameters: waveforms, pulse widths, unison, envelope times, detune
    /// ratios, filter type, string damping/brightness, pitch envelope, velocity and glide
    /// settings. Running voices keep their phase and level; the next tick refreshes
    /// frequencies and filter coefficients.
    pub fn apply_params(&mut self, p: &SynthParams, sample_rate: f32) {
        for o in &mut self.osc1 {
            o.set_wave(p.osc1_wave);
            o.set_pulse_width(p.osc1_pulse);
        }
        self.osc2.set_wave(p.osc2_wave);
        self.osc2.set_pulse_width(p.osc2_pulse);
        self.env
            .set_times(sample_rate, p.attack, p.decay, p.sustain, p.release);
        self.filter_env.set_times(
            sample_rate,
            p.filter_attack,
            p.filter_decay,
            p.filter_sustain,
            p.filter_release,
        );
        self.filter
            .set_type(FilterType::from_index(usize::from(p.filter_type)));
        self.ratio1 = cents_to_ratio(p.osc1_detune);
        self.ratio2 = cents_to_ratio(p.osc2_detune);

        let n = usize::from(p.osc1_unison).clamp(1, MAX_UNISON);
        self.unison_count = n;
        self.unison_gain = 1.0 / (n as f32).sqrt();
        for (i, r) in self.unison_ratio.iter_mut().enumerate() {
            // Symmetric spread: the copies sit at -spread ..= +spread cents (0 for n = 1).
            let cents = if n > 1 && i < n {
                p.osc1_spread * (2.0 * i as f32 / (n as f32 - 1.0) - 1.0)
            } else {
                0.0
            };
            *r = cents_to_ratio(cents);
        }

        self.noise
            .set_color(NoiseColor::from_index(usize::from(p.noise_color)));
        self.fm.set_index(fm_index(p.fm_amount));
        self.pluck_mode = p.is_pluck_mode();
        self.pluck.set_damping(p.pluck_damping);
        self.pluck.set_brightness(p.pluck_bright);

        self.glide_coeff = f64::from(p.glide_coeff(sample_rate));
        if self.glide_coeff >= 1.0 {
            self.pitch = self.pitch_target;
        }
        self.pitch_env_amount = p.pitch_env_amount;
        self.pitch_env_coeff = p.pitch_env_coeff(sample_rate);
        self.key_track = p.filter_key_track;
        self.vel_to_cutoff = p.vel_to_cutoff;
        self.vel_to_amp = p.vel_to_amp;
        self.refresh_note_scalars();
        self.freq_dirty = true;
        self.mod_counter = 0;
    }

    /// Recomputes the per-note constants: cutoff offset (key tracking + velocity) and the
    /// velocity -> amplitude gain.
    fn refresh_note_scalars(&mut self) {
        let note = f32::from(self.note);
        // Velocity is relative to full velocity: a soft note closes the filter by up to
        // VEL_CUTOFF_OCTAVES x VelToCutoff, a full-velocity note leaves the cutoff knob as set
        // (so the default VelToCutoff does not shift every patch's cutoff).
        self.note_octaves = (note - 60.0) / 12.0 * self.key_track
            + (self.velocity - 1.0) * self.vel_to_cutoff * VEL_CUTOFF_OCTAVES;
        self.vel_gain = 1.0 - self.vel_to_amp + self.vel_to_amp * self.velocity;
    }

    /// Starts `note` at `velocity`. A voice that is still sounding keeps its oscillator
    /// phases, filter state, string and envelope levels (click-free retrigger); an idle one
    /// restarts from the waveforms' zero crossings with a cleared string. With
    /// `glide_from = Some(pitch)` (and a non-zero glide time) the pitch slews from that MIDI
    /// pitch to `note`. The pitch envelope restarts and, in pluck mode, the string is
    /// re-excited.
    pub fn note_on(
        &mut self,
        note: u8,
        velocity: f32,
        sample_rate: f32,
        age: u64,
        glide_from: Option<f32>,
    ) {
        let _ = sample_rate;
        if self.env.is_idle() {
            for (i, o) in self.osc1.iter_mut().enumerate() {
                o.reset();
                if i > 0 {
                    o.set_phase(o.phase() + i as f64 * UNISON_PHASE_STEP);
                }
            }
            self.osc2.reset();
            self.sub.reset();
            self.fm.reset();
            self.filter.reset();
            self.filter_env.reset();
            self.pluck.clear();
        }
        self.note = note;
        self.velocity = velocity;
        self.age = age;
        self.pitch_target = f64::from(note);
        self.pitch = match glide_from {
            Some(from) if self.glide_coeff < 1.0 && from.is_finite() => f64::from(from),
            _ => self.pitch_target,
        };
        self.pitch_env = if self.pitch_env_amount.abs() > PITCH_ENV_FLOOR {
            self.pitch_env_amount
        } else {
            0.0
        };
        self.refresh_note_scalars();
        self.pluck_pending = self.pluck_mode;
        self.freq_dirty = true;
        self.mod_counter = 0;
        self.env.note_on();
        self.filter_env.note_on();
    }

    /// Releases the voice (both envelopes enter their release stage).
    pub fn note_off(&mut self) {
        self.env.note_off();
        self.filter_env.note_off();
    }

    /// Tunes every pitched source to `semis` (MIDI semitones, after glide, pitch envelope
    /// and vibrato) and fires a pending pluck.
    #[inline]
    fn tune(&mut self, p: &SynthParams, semis: f32) {
        let hz = note_to_hz(semis);
        let hz1 = hz * self.ratio1;
        if self.pluck_mode {
            self.pluck.set_freq(hz1);
            if self.pluck_pending {
                self.pluck.pluck();
                self.pluck_pending = false;
            }
        } else {
            let n = self.unison_count;
            for (o, r) in self.osc1[..n].iter_mut().zip(&self.unison_ratio) {
                o.set_freq(hz1 * r);
            }
            self.osc2.set_freq(hz * self.ratio2);
            self.sub.set_freq(hz1 * 0.5);
            self.fm.set_freq(hz1 * p.fm_ratio);
        }
        self.tuned_semis = semis;
    }

    /// The subtractive source mix: unison osc 1 (with FM), osc 2, sub and noise.
    #[inline]
    fn subtractive(&mut self, p: &SynthParams) -> f32 {
        let n = self.unison_count;
        let mut o1 = 0.0;
        if self.fm.is_off() {
            for o in &mut self.osc1[..n] {
                o1 += o.tick();
            }
        } else {
            let pm = self.fm.tick();
            for o in &mut self.osc1[..n] {
                o1 += o.tick_pm(pm);
            }
        }
        let mut mix = o1 * self.unison_gain * p.osc1_mix + self.osc2.tick() * p.osc2_mix;
        if p.sub_level > 0.0 {
            mix += self.sub.tick() * p.sub_level;
        }
        if p.noise_level > 0.0 {
            mix += self.noise.tick() * p.noise_level;
        }
        mix
    }

    /// Produces the next sample (already scaled by velocity and [`VOICE_LEVEL_DB`]) using the
    /// pool's parameters `p` and the shared LFO value `lfo` (`-1.0..=1.0`).
    /// Returns exactly `0.0` for an idle voice without advancing anything.
    #[inline]
    pub fn tick(&mut self, p: &SynthParams, lfo: f32, voice_gain: f32) -> f32 {
        if self.env.is_idle() {
            return 0.0;
        }
        let env = self.env.tick();
        let fenv = self.filter_env.tick();

        // Pitch: glide and pitch envelope advance every sample; the oscillators are retuned
        // immediately when dirty, otherwise every MOD_INTERVAL samples while something moves.
        let mut moving = false;
        if self.pitch != self.pitch_target {
            let d = self.pitch_target - self.pitch;
            if d.abs() < 1.0e-3 || self.glide_coeff >= 1.0 {
                self.pitch = self.pitch_target;
            } else {
                self.pitch += self.glide_coeff * d;
            }
            moving = true;
        }
        if self.pitch_env != 0.0 {
            self.pitch_env *= self.pitch_env_coeff;
            if self.pitch_env.abs() < PITCH_ENV_FLOOR {
                self.pitch_env = 0.0;
            }
            moving = true;
        }
        let lfo_pitch = p.lfo_target == LfoTarget::Pitch && p.lfo_depth > 0.0;
        let refresh = self.mod_counter == 0;
        if self.freq_dirty || (refresh && (moving || lfo_pitch)) {
            let mut semis = self.pitch as f32 + self.pitch_env;
            if lfo_pitch {
                semis += lfo * p.lfo_depth * LFO_PITCH_SEMITONES;
            }
            if self.freq_dirty || (semis - self.tuned_semis).abs() > PITCH_EPSILON {
                self.tune(p, semis);
            }
            self.freq_dirty = false;
        }

        // Cutoff: base * 2^(filter env * amount * 4 + key tracking + velocity + lfo * 2 *
        // depth), refreshed every MOD_INTERVAL samples (the filter skips the recompute when
        // nothing changed).
        if refresh {
            let mut octaves = fenv * p.env_amount * ENV_CUTOFF_OCTAVES + self.note_octaves;
            if p.lfo_target == LfoTarget::Cutoff {
                octaves += lfo * p.lfo_depth * LFO_CUTOFF_OCTAVES;
            }
            self.filter.set(p.cutoff * octaves.exp2(), p.resonance);
            self.mod_counter = MOD_INTERVAL;
        }
        self.mod_counter -= 1;

        let src = if self.pluck_mode {
            self.pluck.tick()
        } else {
            self.subtractive(p)
        };
        self.filter.tick(src) * env * self.vel_gain * voice_gain
    }
}

/// Fixed-size polyphonic voice pool with voice stealing, the shared LFO and the current
/// [`SynthParams`].
///
/// Allocation order for `note_on`: the voice already playing that note (retrigger), else an
/// idle voice, else the oldest releasing voice, else the oldest held voice. Allocation-free
/// after construction; all voices live in a fixed array.
#[derive(Debug, Clone, PartialEq)]
pub struct VoicePool {
    voices: [Voice; MAX_VOICES],
    sample_rate: f32,
    voice_gain: f32,
    /// Monotonic counter stamped onto voices at `note_on`.
    counter: u64,
    params: SynthParams,
    lfo: Lfo,
}

impl VoicePool {
    /// Creates a pool of idle voices with the default synth parameters.
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let params = SynthParams::default();
        let env = Adsr::new(
            sample_rate,
            params.attack,
            params.decay,
            params.sustain,
            params.release,
        );
        Self {
            voices: core::array::from_fn(|i| Voice::with_seed(sample_rate, env, i as u32 + 1)),
            sample_rate,
            voice_gain: db_to_gain(VOICE_LEVEL_DB),
            counter: 0,
            params,
            lfo: Lfo::new(sample_rate, params.lfo_rate),
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

    /// The current synth parameters.
    #[must_use]
    pub fn params(&self) -> &SynthParams {
        &self.params
    }

    /// The shared LFO (for inspection).
    #[must_use]
    pub fn lfo(&self) -> &Lfo {
        &self.lfo
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
        let p = &self.params;
        (p.attack, p.decay, p.sustain, p.release)
    }

    /// Sets the amplitude envelope for every voice, running ones included (their current level
    /// is preserved; only the remaining stages change speed). Values are clamped to the
    /// corresponding [`ParamId`] ranges.
    pub fn set_adsr(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        let mut p = self.params;
        p.set(ParamId::AmpAttack, attack);
        p.set(ParamId::AmpDecay, decay);
        p.set(ParamId::AmpSustain, sustain);
        p.set(ParamId::AmpRelease, release);
        self.set_params(&p);
    }

    /// Replaces every synth parameter (clamped) and applies them to all voices and the shared
    /// LFO. Running voices keep their phase and level.
    pub fn set_params(&mut self, params: &SynthParams) {
        self.params = params.clamped();
        self.lfo.set_rate(self.params.lfo_rate);
        let sr = self.sample_rate;
        for v in &mut self.voices {
            v.apply_params(&self.params, sr);
        }
    }

    /// Sets a single synth parameter and applies it. Returns `false` (changing nothing) for
    /// ids that are not synth parameters.
    pub fn set_param(&mut self, id: ParamId, value: f32) -> bool {
        let mut p = self.params;
        if !p.set(id, value) {
            return false;
        }
        self.set_params(&p);
        true
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

    /// Pitch to glide from: the current pitch of the most recently started held voice, when
    /// glide is on and some key is still held (legato).
    fn glide_source(&self) -> Option<f32> {
        if self.params.glide <= 0.0 {
            return None;
        }
        self.voices
            .iter()
            .filter(|v| v.state() == VoiceState::Held)
            .max_by_key(|v| v.age)
            .map(Voice::pitch)
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
        let glide_from = self.glide_source();
        let i = self.allocate(note);
        self.counter = self.counter.wrapping_add(1);
        let age = self.counter;
        self.voices[i].note_on(note, velocity, self.sample_rate, age, glide_from);
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

    /// Silences every voice instantly and restarts the LFO. Only for resets while the output
    /// is already silent.
    pub fn reset(&mut self) {
        for v in &mut self.voices {
            v.env.reset();
            v.filter_env.reset();
            v.filter.reset();
            v.pluck.clear();
            v.pitch_env = 0.0;
        }
        self.lfo.reset();
    }

    /// Produces the next mono sample: the sum of all sounding voices. The shared LFO advances
    /// once per call.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let lfo = self.lfo.tick();
        let gain = self.voice_gain;
        let p = &self.params;
        self.voices.iter_mut().map(|v| v.tick(p, lfo, gain)).sum()
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

    /// One plain sine, filter wide open, no modulation: the M1 voice.
    fn sine_patch(pool: &mut VoicePool) {
        let mut p = *pool.params();
        p.osc1_wave = Wave::Sine;
        p.osc1_mix = 1.0;
        p.osc2_mix = 0.0;
        p.cutoff = 20_000.0;
        p.resonance = 0.0;
        p.env_amount = 0.0;
        p.lfo_depth = 0.0;
        pool.set_params(&p);
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
        for pool in [&mut a, &mut b] {
            sine_patch(pool);
            pool.set_adsr(0.001, 0.01, 1.0, 0.1);
        }
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
    fn set_params_round_trips_and_clamps() {
        let mut pool = VoicePool::new(SR);
        let mut p = SynthParams::default();
        assert!(p.set(ParamId::FilterCutoff, 1.0e9));
        assert_eq!(p.cutoff, 20_000.0);
        assert!(p.set(ParamId::Osc2Wave, 2.4));
        assert_eq!(p.osc2_wave, Wave::Square);
        assert!(!p.set(ParamId::DelayMix, 0.5));
        assert_eq!(p.get(ParamId::DelayMix), None);
        assert_eq!(p.get(ParamId::Osc2Wave), Some(2.0));
        // M8 ids: every synth id is stored and read back; discrete ones round to an index.
        let mut m8 = SynthParams::default();
        for id in ParamId::ALL.iter().filter(|id| id.is_synth()) {
            let r = id.range();
            assert!(m8.set(*id, *r.end()), "{id:?}");
            assert_eq!(m8.get(*id), Some(*r.end()), "{id:?}");
            assert!(m8.set(*id, id.default_value()), "{id:?}");
            assert_eq!(m8.get(*id), Some(id.default_value()), "{id:?}");
        }
        assert_eq!(m8, SynthParams::default());
        assert!(m8.set(ParamId::Osc1Unison, 7.4));
        assert_eq!(m8.osc1_unison, 7);
        assert_eq!(m8.get(ParamId::Osc1Unison), Some(7.0));
        assert!(m8.set(ParamId::FilterType, 1.6));
        assert_eq!(m8.filter_type, 2);
        assert!(m8.set(ParamId::OscMode, 0.4));
        assert_eq!(m8.osc_mode, 0);
        assert!(m8.set(ParamId::PitchEnvAmount, -99.0));
        assert_eq!(m8.pitch_env_amount, -24.0);
        assert!(!m8.set(ParamId::SidechainAmount, 0.5));
        assert_eq!(m8.get(ParamId::SidechainAmount), None);
        assert_eq!(m8.get(ParamId::LofiMix), None);
        p.glide = f32::NAN;
        pool.set_params(&p);
        assert_eq!(pool.params().glide, ParamId::Glide.default_value());
        assert_eq!(pool.params().cutoff, 20_000.0);
        assert!(pool.set_param(ParamId::LfoRate, 7.0));
        assert_eq!(pool.lfo().rate(), 7.0);
        assert!(!pool.set_param(ParamId::MasterGain, 2.0));
        let values = [0.5_f32; ParamId::COUNT];
        let from_values = SynthParams::from_values(&values);
        assert_eq!(from_values.cutoff, 20.0, "clamped up to the range start");
        assert_eq!(from_values.osc1_wave, Wave::Saw, "0.5 rounds to 1");
    }

    #[test]
    fn glide_slews_pitch_only_while_legato() {
        let mut pool = VoicePool::new(SR);
        pool.set_param(ParamId::Glide, 0.2);
        pool.note_on(60, 1.0);
        run(&mut pool, 100);
        pool.note_on(72, 1.0);
        let v = pool.voices().iter().find(|v| v.note() == 72).unwrap();
        assert!((v.pitch() - 60.0).abs() < 0.01, "starts at the held pitch");
        run(&mut pool, 2400);
        let v = pool.voices().iter().find(|v| v.note() == 72).unwrap();
        assert!(
            v.pitch() > 62.0 && v.pitch() < 71.0,
            "mid-glide {}",
            v.pitch()
        );
        run(&mut pool, 48_000);
        let v = pool.voices().iter().find(|v| v.note() == 72).unwrap();
        assert!((v.pitch() - 72.0).abs() < 0.01, "arrived {}", v.pitch());

        // No key held: the next note starts at its own pitch.
        pool.all_notes_off();
        run(&mut pool, 48_000);
        pool.note_on(48, 1.0);
        let v = pool.voices().iter().find(|v| v.note() == 48).unwrap();
        assert_eq!(v.pitch(), 48.0);

        // Glide off: immediate even when legato.
        pool.set_param(ParamId::Glide, 0.0);
        pool.note_on(55, 1.0);
        let v = pool.voices().iter().find(|v| v.note() == 55).unwrap();
        assert_eq!(v.pitch(), 55.0);
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
        pool.reset();
        assert!(!pool.is_active());
    }
}
