//! Polyphonic voice allocation and the per-voice synth signal path.
//!
//! Voices are pre-allocated in `new_engine`; the pool never grows on the audio thread.
//! Each [`Voice`] (M2) is two band-limited [`Oscillator`]s (wave, detune, mix) into a lowpass
//! [`Svf`] whose cutoff follows the amp [`Adsr`] (filter envelope amount) and the pool's
//! shared [`Lfo`], with optional glide between notes. The [`VoicePool`] owns [`MAX_VOICES`]
//! voices in a fixed array, the shared LFO and the current [`SynthParams`], and sums the
//! sounding voices per sample.
//!
//! **Level:** one oscillator at mix `1.0` and velocity `1.0` peaks at [`VOICE_LEVEL_DB`]
//! (-12 dBFS, same as the M0 test tone) before the filter. The default patch mixes two
//! oscillators (0.7 + 0.45), so a 16-voice chord sums to roughly +13 dBFS before the master
//! gain, which the master limiter squashes softly instead of clipping.
//!
//! **Modulation rate:** oscillator frequencies are recomputed only while a voice glides or the
//! LFO targets pitch; the filter coefficients are refreshed every [`MOD_INTERVAL`] samples
//! (and skipped when nothing changed), which keeps the per-sample cost to the two oscillators,
//! the filter and the envelope.

use crate::dsp::db_to_gain;
use crate::params::{Adsr, ParamId};
use crate::synth::{Lfo, LfoTarget, Oscillator, Svf, Wave, cents_to_ratio, note_to_hz};

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

/// The glide parameter is the time for the pitch to cover 95 % of the jump (three time
/// constants of the one-pole slew).
const GLIDE_TIME_CONSTANTS: f32 = 3.0;

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

/// One synth voice: two oscillators, lowpass filter, note, velocity, amplitude envelope,
/// glide state and an age stamp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Voice {
    osc1: Oscillator,
    osc2: Oscillator,
    filter: Svf,
    env: Adsr,
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
    /// Set when the oscillator frequencies must be recomputed on the next tick.
    freq_dirty: bool,
    /// Countdown to the next filter coefficient update.
    mod_counter: u8,
}

impl Voice {
    /// Creates an idle voice for the given sample rate with the envelope `env` and the
    /// default synth parameters.
    #[must_use]
    pub fn new(sample_rate: f32, env: Adsr) -> Self {
        let p = SynthParams::default();
        let hz = note_to_hz(69.0);
        let mut v = Self {
            osc1: Oscillator::new(p.osc1_wave, hz, sample_rate),
            osc2: Oscillator::new(p.osc2_wave, hz, sample_rate),
            filter: Svf::new(sample_rate, p.cutoff, p.resonance),
            env,
            note: 0,
            velocity: 0.0,
            age: 0,
            pitch: 69.0,
            pitch_target: 69.0,
            glide_coeff: 1.0,
            ratio1: 1.0,
            ratio2: 1.0,
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

    /// The lowpass filter (for inspection: current cutoff after modulation).
    #[must_use]
    pub fn filter(&self) -> &Svf {
        &self.filter
    }

    /// Applies new synth parameters: waveforms, envelope times, detune ratios and glide
    /// speed. Running voices keep their phase and level; the next tick refreshes frequencies
    /// and filter coefficients.
    pub fn apply_params(&mut self, p: &SynthParams, sample_rate: f32) {
        self.osc1.set_wave(p.osc1_wave);
        self.osc2.set_wave(p.osc2_wave);
        self.env
            .set_times(sample_rate, p.attack, p.decay, p.sustain, p.release);
        self.ratio1 = cents_to_ratio(p.osc1_detune);
        self.ratio2 = cents_to_ratio(p.osc2_detune);
        self.glide_coeff = f64::from(p.glide_coeff(sample_rate));
        if self.glide_coeff >= 1.0 {
            self.pitch = self.pitch_target;
        }
        self.freq_dirty = true;
        self.mod_counter = 0;
    }

    /// Starts `note` at `velocity`. A voice that is still sounding keeps its oscillator
    /// phases, filter state and envelope level (click-free retrigger); an idle one restarts
    /// from the waveforms' zero crossings. With `glide_from = Some(pitch)` (and a non-zero
    /// glide time) the pitch slews from that MIDI pitch to `note`.
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
            self.osc1.reset();
            self.osc2.reset();
            self.filter.reset();
        }
        self.note = note;
        self.velocity = velocity;
        self.age = age;
        self.pitch_target = f64::from(note);
        self.pitch = match glide_from {
            Some(from) if self.glide_coeff < 1.0 && from.is_finite() => f64::from(from),
            _ => self.pitch_target,
        };
        self.freq_dirty = true;
        self.mod_counter = 0;
        self.env.note_on();
    }

    /// Releases the voice (envelope enters its release stage).
    pub fn note_off(&mut self) {
        self.env.note_off();
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

        // Pitch: glide, then LFO vibrato; only recompute frequencies when something moves.
        let mut gliding = false;
        if self.pitch != self.pitch_target {
            let d = self.pitch_target - self.pitch;
            if d.abs() < 1.0e-3 || self.glide_coeff >= 1.0 {
                self.pitch = self.pitch_target;
            } else {
                self.pitch += self.glide_coeff * d;
            }
            gliding = true;
        }
        let lfo_pitch = p.lfo_target == LfoTarget::Pitch && p.lfo_depth > 0.0;
        if gliding || lfo_pitch || self.freq_dirty {
            let mut semis = self.pitch as f32;
            if lfo_pitch {
                semis += lfo * p.lfo_depth * LFO_PITCH_SEMITONES;
            }
            let hz = note_to_hz(semis);
            self.osc1.set_freq(hz * self.ratio1);
            self.osc2.set_freq(hz * self.ratio2);
            self.freq_dirty = false;
        }

        // Cutoff: base * 2^(env * amount * 4) * 2^(lfo * 2 * depth), refreshed every
        // MOD_INTERVAL samples (the filter skips the recompute when nothing changed).
        if self.mod_counter == 0 {
            let mut octaves = env * p.env_amount * ENV_CUTOFF_OCTAVES;
            if p.lfo_target == LfoTarget::Cutoff {
                octaves += lfo * p.lfo_depth * LFO_CUTOFF_OCTAVES;
            }
            self.filter.set(p.cutoff * octaves.exp2(), p.resonance);
            self.mod_counter = MOD_INTERVAL;
        }
        self.mod_counter -= 1;

        let mix = self.osc1.tick() * p.osc1_mix + self.osc2.tick() * p.osc2_mix;
        self.filter.tick(mix) * env * self.velocity * voice_gain
    }
}

/// Fixed-size polyphonic voice pool with voice stealing, the shared LFO and the current
/// [`SynthParams`].
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
            voices: [Voice::new(sample_rate, env); MAX_VOICES],
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
            v.filter.reset();
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
