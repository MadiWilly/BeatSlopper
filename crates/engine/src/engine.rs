//! Engine core: message queue, polyphonic voices, drum kit, step sequencer, mixer, test tone,
//! smoothed master gain and the master limiter.
//!
//! The public API below is the contract the `app` crate builds against (docs/ARCHITECTURE.md).
//!
//! # Signal flow (M6)
//!
//! Per sample, six mixer channels ([`Channel`]): the five drum rows from
//! [`DrumKit::tick_rows`] and the synth voices' sum, the latter through the drive insert
//! (`DriveAmount` / `DriveMix` of the FX chain). Each channel is scaled by its smoothed gain
//! (fader level times the mute/solo decision, [`MixerState::effective_gain`]), panned
//! ([`crate::mixer::pan_gains`], smoothed) into the stereo dry bus, and its post-fader signal
//! is also sent (smoothed send levels, same pan) into a stereo delay bus and a stereo reverb
//! bus. [`FxChain::process_bus`] returns `dry + delay * DelayMix + reverb * ReverbMix`; then
//! `(+ TestTone) -> master gain (smoothed) -> Limiter (bypassable via LimiterOn) ->
//! interleaved stereo`. The diagnostic test tone bypasses the mixer and the effects.
//!
//! With the default [`MixerState`] (every strip at unity and centre, the synth sent fully to
//! both effects) the synth is driven, delayed and reverberated as in M2-M5 and the dry
//! levels are unchanged (the pan law is unity at the centre). Differences from the pre-mixer
//! engine: the drums no longer pass through the drive (it is the synth's insert), the delay
//! output no longer feeds the reverb (the buses are parallel), and `DelayMix` / `ReverbMix`
//! are return levels rather than dry/wet crossfades, so the dry synth is no longer attenuated
//! by `(1 - mix)` (about +4 dB dry at the defaults, with the same wet level).
//!
//! Block peaks of every channel (post-fader) and of the master output are published once per
//! `render` as [`Meters`] through lock-free atomics ([`EngineHandle::meters`]).
//!
//! # Sequencer (M3, polyphonic notes since M7)
//!
//! The scheduler runs per sample inside [`Engine::render`], so triggers are sample-accurate
//! and independent of the block size. While the transport plays, `pos` counts samples from
//! the start of the 16-step bar; step `n` fires when `pos` reaches
//! [`Transport::step_trigger_sample`] (`round((n + swing_offset) * samples_per_step)`), and the
//! bar wraps to `pos = 0` after `round(16 * samples_per_step)` samples, where step 0 fires
//! again. Drum steps call [`DrumKit::trigger`]. Synth notes ([`Note`]) of the playing pattern
//! start when their `start` step (`bar * 16 + step`) fires: `voices.note_on(pitch, velocity)`
//! for every note starting there (chords are simply several notes), and a note-off is
//! scheduled for **the trigger sample of step `start + length` minus one sample** (with that
//! step's swing offset), so a length-1 note on step 0 at 120 BPM / 48 kHz sounds for samples
//! `0..=5998`, a note of length 3 on step 2 for `12 000..=29 998`, and a note that ends where
//! another of the same pitch starts hands its voice over cleanly (one release sample, then the
//! retrigger). A note that runs past the pattern end is released at the pattern's last
//! sample (its wrap); a pattern switch, song advance, stop or play releases every active
//! sequencer note. Active sequencer notes live in a fixed array of `(pitch, off sample)` pairs,
//! one per voice ([`MAX_VOICES`]); a pitch is tracked once (a new note on a sounding pitch
//! releases and reuses its slot) and when the array is full the note ending soonest is cut,
//! so polyphony is bounded by the voice pool's stealing. Sequencer notes are tracked
//! separately from keyboard notes so a held key is never cut by the sequencer. A tempo change
//! while playing keeps the fractional position in the bar, so the loop neither jumps nor
//! skips or repeats steps (pending note-offs keep their sample positions).
//!
//! # Patterns and song mode (M5)
//!
//! The engine owns a [`Bank`]: eight patterns, the chain, the play mode and the selected
//! (`current`) pattern. Two indices matter on the audio thread: the *edit* pattern (where
//! step/row/volume/tune messages land, [`Engine::edit_pattern`]) and the *playing* pattern
//! ([`Position::pattern`]). The bar is always 16 steps; the step fired is `bar * 16 + step`
//! of the playing pattern. Every time `pos` wraps, the bar-boundary logic runs:
//!
//! - pattern mode: a queued [`Message::SelectPattern`] switches the playing pattern now
//!   (bar 0); otherwise, once `bar` reaches the playing pattern's bar count, the pattern
//!   restarts from bar 0 (as the selected pattern). A length change therefore applies at the
//!   next bar boundary, because the bar count is re-read there.
//! - song mode (non-empty chain): once the pattern's bars are done one repeat of the entry
//!   is consumed; when the repeats are used up the chain advances (wrapping at `chain_len`)
//!   and the new entry's pattern starts at bar 0. Switching into song mode while playing
//!   restarts the chain from entry 0 at the next bar boundary. An empty chain behaves like
//!   pattern mode on the selected pattern.
//!
//! [`Engine::play`] always starts at step 0 of the current pattern (song mode: chain entry
//! 0). Nothing here allocates: the bank is fixed-size, pattern copies are plain `memcpy`s and
//! [`Engine::set_bank`] (a project load) is meant to be called before the stream starts.
//!
//! Real-time rules: everything used by [`Engine::render`] is pre-allocated in [`new_engine`].
//! `render` never allocates, locks, does I/O, logs or panics; it tolerates `out` buffers that
//! are shorter than `frames * 2` by rendering what fits.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use rtrb::{Consumer, Producer, RingBuffer};

use crate::drums::{DrumKit, DrumRow};
use crate::dsp::db_to_gain;
use crate::fx::{EqParam, EqState, FxChain, Limiter, ParametricEq, ReverbParam, ReverbState};
use crate::mixer::{Channel, ChannelParam, MixerState};
use crate::params::{DEFAULT_SMOOTH_MS, ParamId, Smoother};
use crate::pattern::{
    Bank, ChainEntry, DrumStep, MAX_CHAIN, MAX_STEPS, Note, PATTERN_COUNT, Pattern, PlayMode, STEPS,
};
use crate::spectrum::{SpectrumReader, SpectrumTap, spectrum_pair};
use crate::synth::SineOsc;
use crate::transport::Transport;
use crate::voice::{MAX_VOICES, SynthParams, VoicePool};

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
/// Smoothing time constant of the mixer's channel gains, pans and sends, in milliseconds.
pub const MIXER_SMOOTH_MS: f32 = DEFAULT_SMOOTH_MS;
/// Largest master gain accepted by [`Engine::set_master_gain`] (+12 dB).
pub const MAX_MASTER_GAIN: f32 = 4.0;
/// Gate length of an M3 sequencer synth-lane note as a fraction of one step. Unused since M7:
/// a note now sounds until the trigger sample of step `start + length` minus one sample (see
/// the module docs), so a length-1 note fills its whole step.
#[deprecated(
    since = "0.1.0",
    note = "M7 notes end one sample before the trigger of step `start + length`; there is no gate fraction"
)]
pub const SEQ_GATE: f64 = 0.9;

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
    /// Start the sequencer from step 0 of the current pattern (song mode: chain entry 0).
    Play,
    /// Stop the sequencer, release its notes and rewind to step 0.
    Stop,
    /// Swing amount `0.0..=0.75`: odd steps are delayed by that fraction of a step.
    SetSwing(f32),
    /// Set one drum step of the edit pattern.
    SetDrumStep {
        /// Drum row.
        row: DrumRow,
        /// Step index `0..length` of the edit pattern (wraps).
        step: u8,
        /// Whether the step fires.
        on: bool,
        /// Accented (full velocity).
        accent: bool,
    },
    /// Set one synth-lane step of the edit pattern.
    SetSynthStep {
        /// Step index `0..length` of the edit pattern (wraps).
        step: u8,
        /// MIDI note, or `None` for a rest.
        note: Option<u8>,
        /// Accented (full velocity).
        accent: bool,
    },
    /// Clear one drum row of the edit pattern.
    ClearRow(DrumRow),
    /// Clear the synth lane of the edit pattern.
    ClearSynth,
    /// Clear every row and the synth lane of the edit pattern.
    ClearAll,
    /// Per-row drum volume in dB (-24..=6) of the edit pattern.
    SetDrumVolume {
        /// Drum row.
        row: DrumRow,
        /// Volume in dB.
        db: f32,
    },
    /// Per-row drum tune in semitones (-12..=12) of the edit pattern.
    SetDrumTune {
        /// Drum row.
        row: DrumRow,
        /// Tune in semitones.
        semitones: f32,
    },
    /// Choose which pattern the step/row/volume/tune messages edit (`0..PATTERN_COUNT`).
    /// Does not change what plays.
    SetEditPattern(u8),
    /// Set a pattern's length to 16, 32 or 64 steps (anything else rounds up to the next
    /// allowed length). For the playing pattern this applies at its next bar boundary.
    SetPatternLength {
        /// Pattern index `0..PATTERN_COUNT`.
        pattern: u8,
        /// Length in steps.
        steps: u8,
    },
    /// Copy pattern `from` over pattern `to`.
    CopyPattern {
        /// Source pattern index.
        from: u8,
        /// Destination pattern index.
        to: u8,
    },
    /// Clear every step of a pattern (length, volume and tune are kept).
    ClearPattern(u8),
    /// Select a pattern: it becomes the edit pattern and the selected (`current`) one. In
    /// pattern mode while playing the switch is queued to the next 16-step bar boundary
    /// ([`Position::queued`]); when stopped it is immediate.
    SelectPattern(u8),
    /// Switch between pattern and song mode. While playing, song mode starts the chain from
    /// entry 0 at the next bar boundary; pattern mode queues the selected pattern.
    SetPlayMode(PlayMode),
    /// Set chain entry `index` (`0..MAX_CHAIN`); the chain grows to include it.
    SetChainEntry {
        /// Chain position.
        index: u8,
        /// Pattern index `0..PATTERN_COUNT`.
        pattern: u8,
        /// Repeat count `1..=8`.
        repeats: u8,
    },
    /// Set the number of valid chain entries (`0..=MAX_CHAIN`).
    SetChainLen(u8),
    /// Set or add a synth note of pattern `pattern` (M7): `index < note_count` replaces that
    /// note, `index == note_count` appends, anything larger is ignored, as is a note with
    /// `length == 0` or a `start` at or beyond the pattern length. The note list stays sorted
    /// (so the note's index may change) and the grid lane is rebuilt.
    SetNote {
        /// Pattern index `0..PATTERN_COUNT`.
        pattern: u8,
        /// Position in the sorted note list, `0..=note_count`.
        index: u8,
        /// The note (clamped into range).
        note: Note,
    },
    /// Remove synth note `index` of pattern `pattern` (out-of-range indices are ignored).
    RemoveNote {
        /// Pattern index `0..PATTERN_COUNT`.
        pattern: u8,
        /// Position in the sorted note list, `0..note_count`.
        index: u8,
    },
    /// Remove every synth note of a pattern (same as `ClearSynth` for that pattern).
    ClearNotes(u8),
    /// Set one mixer value; it is clamped with [`ChannelParam::clamp`] (mute/solo snap to
    /// 0/1) and smoothed over about [`MIXER_SMOOTH_MS`].
    SetMixer {
        /// Channel strip.
        channel: Channel,
        /// Which value of the strip.
        param: ChannelParam,
        /// New value (dB for `Level`, `-1..=1` for `Pan`, `0..=1` otherwise).
        value: f32,
    },
    /// Set one value of a master EQ band (plugins milestone); clamped by the band, smoothed
    /// over `fx::eq::EQ_SMOOTH_MS`.
    SetEq {
        /// Band index `0..BAND_COUNT` (wraps).
        band: u8,
        /// Which value.
        param: EqParam,
        /// New value (Hz / dB / Q, `0`/`1` for `On`, a kind index for `Kind`).
        value: f32,
    },
    /// Switch the whole master EQ on or off (faded).
    SetEqOn(bool),
    /// Master EQ output gain in dB (`-18..=18`).
    SetEqGain(f32),
    /// Set one reverb value (plugins milestone). `Size` / `Damping` / `Mix` are the same
    /// values as `ParamId::ReverbSize` / `ReverbDamping` / `ReverbMix`; either path updates both views.
    SetReverb {
        /// Which value.
        param: ReverbParam,
        /// New value (clamped by [`ReverbParam::clamp`]).
        value: f32,
    },
}

/// Block peak levels (linear, `0.0` = silence) as last published by the audio thread.
///
/// Each value is the largest absolute sample of the last `render` call: per channel the
/// post-fader (pre-pan) signal, for the master the final left/right output. The UI applies
/// its own decay / hold.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Meters {
    /// Per-channel peaks indexed by [`Channel::index`].
    pub channel: [f32; Channel::COUNT],
    /// Master output peaks, left and right.
    pub master: [f32; 2],
}

impl Meters {
    /// All meters at silence.
    pub const SILENT: Meters = Meters {
        channel: [0.0; Channel::COUNT],
        master: [0.0; 2],
    };

    /// Peak of `channel`.
    #[must_use]
    pub fn of(&self, channel: Channel) -> f32 {
        self.channel[channel.index()]
    }

    /// The larger of the two master peaks.
    #[must_use]
    pub fn master_peak(&self) -> f32 {
        self.master[0].max(self.master[1])
    }

    /// Replaces any NaN with `0.0` and clamps negatives (defensive, for values read from
    /// atomics).
    fn sanitised(mut self) -> Self {
        for v in self.channel.iter_mut().chain(self.master.iter_mut()) {
            *v = if v.is_finite() && *v > 0.0 { *v } else { 0.0 };
        }
        self
    }
}

/// Where the sequencer is, as last published by the audio thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    /// Current step within the bar, `0..16`.
    pub step: u8,
    /// Fraction of the current step already elapsed, `0.0..1.0`.
    pub frac: f32,
    /// Whether the transport is running.
    pub playing: bool,
    /// The pattern that is playing (or will play on the next `Play`), `0..PATTERN_COUNT`.
    pub pattern: u8,
    /// 0-based bar within the playing pattern (`0..pattern bars`).
    pub bar: u8,
    /// Chain entry being played (song mode; `0` otherwise).
    pub song_index: u8,
    /// Pattern queued by `SelectPattern` for the next bar boundary (pattern mode).
    pub queued: Option<u8>,
}

impl Position {
    /// A stopped position at step 0 of pattern `pattern`.
    #[must_use]
    pub const fn stopped(pattern: u8) -> Self {
        Self {
            step: 0,
            frac: 0.0,
            playing: false,
            pattern,
            bar: 0,
            song_index: 0,
            queued: None,
        }
    }

    /// Step index within the playing pattern: `bar * 16 + step`.
    #[must_use]
    pub fn pattern_step(&self) -> usize {
        usize::from(self.bar) * STEPS + usize::from(self.step)
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::stopped(0)
    }
}

/// Marker for "no queued pattern" in the packed position word.
const NO_QUEUE: u32 = 0xFF;

/// Packs `pattern | bar << 8 | song_index << 16 | queued << 24` (`0xFF` = none).
fn pack_song(pattern: u8, bar: u8, song_index: u8, queued: Option<u8>) -> u32 {
    u32::from(pattern)
        | u32::from(bar) << 8
        | u32::from(song_index) << 16
        | queued.map_or(NO_QUEUE, u32::from) << 24
}

/// Inverse of [`pack_song`].
fn unpack_song(word: u32) -> (u8, u8, u8, Option<u8>) {
    let queued = word >> 24;
    (
        (word & 0xFF) as u8,
        ((word >> 8) & 0xFF) as u8,
        ((word >> 16) & 0xFF) as u8,
        (queued != NO_QUEUE).then_some(queued as u8),
    )
}

/// Lock-free playhead state shared between [`Engine`] and [`EngineHandle`].
#[derive(Debug, Default)]
pub(crate) struct Shared {
    step: AtomicU32,
    /// Fraction of the step in thousandths.
    frac_milli: AtomicU32,
    playing: AtomicBool,
    /// Pattern, bar, song index and queued pattern, packed by [`pack_song`].
    song: AtomicU32,
    /// Channel block peaks as `f32` bit patterns.
    meter_channel: [AtomicU32; Channel::COUNT],
    /// Master block peaks (left, right) as `f32` bit patterns.
    meter_master: [AtomicU32; 2],
    /// Block peak of the reverb return (plugins milestone) as an `f32` bit pattern.
    meter_reverb: AtomicU32,
}

impl Shared {
    pub(crate) fn publish_meters(&self, meters: &Meters) {
        for (slot, &v) in self.meter_channel.iter().zip(&meters.channel) {
            slot.store(v.to_bits(), Ordering::Relaxed);
        }
        for (slot, &v) in self.meter_master.iter().zip(&meters.master) {
            slot.store(v.to_bits(), Ordering::Relaxed);
        }
    }

    pub(crate) fn publish_reverb_level(&self, peak: f32) {
        self.meter_reverb.store(peak.to_bits(), Ordering::Relaxed);
    }

    pub(crate) fn read_reverb_level(&self) -> f32 {
        let v = f32::from_bits(self.meter_reverb.load(Ordering::Relaxed));
        if v.is_finite() && v > 0.0 { v } else { 0.0 }
    }

    pub(crate) fn read_meters(&self) -> Meters {
        let mut m = Meters::SILENT;
        for (v, slot) in m.channel.iter_mut().zip(&self.meter_channel) {
            *v = f32::from_bits(slot.load(Ordering::Relaxed));
        }
        for (v, slot) in m.master.iter_mut().zip(&self.meter_master) {
            *v = f32::from_bits(slot.load(Ordering::Relaxed));
        }
        m.sanitised()
    }

    pub(crate) fn publish(&self, pos: Position) {
        self.step.store(u32::from(pos.step), Ordering::Relaxed);
        let milli = (pos.frac.clamp(0.0, 0.999) * 1000.0) as u32;
        self.frac_milli.store(milli, Ordering::Relaxed);
        self.song.store(
            pack_song(pos.pattern, pos.bar, pos.song_index, pos.queued),
            Ordering::Relaxed,
        );
        self.playing.store(pos.playing, Ordering::Relaxed);
    }

    pub(crate) fn read(&self) -> Position {
        let (pattern, bar, song_index, queued) = unpack_song(self.song.load(Ordering::Relaxed));
        Position {
            step: (self.step.load(Ordering::Relaxed) % STEPS as u32) as u8,
            frac: self.frac_milli.load(Ordering::Relaxed) as f32 / 1000.0,
            playing: self.playing.load(Ordering::Relaxed),
            pattern,
            bar,
            song_index,
            queued,
        }
    }
}

/// UI-side handle: sends [`Message`]s to the engine over a lock-free SPSC queue.
pub struct EngineHandle {
    producer: Producer<Message>,
    shared: Arc<Shared>,
    /// The MIDI-thread producer, until [`EngineHandle::take_midi_handle`] hands it out.
    midi: Option<Producer<Message>>,
    /// Reader of the master-output spectrum ring (plugins milestone).
    spectrum: SpectrumReader,
}

/// A second, independent lock-free queue into the engine for a MIDI input thread, so MIDI
/// notes do not wait for the UI frame loop. Obtained once from
/// [`EngineHandle::take_midi_handle`]; `Send`, meant to move to the MIDI thread.
pub struct MidiHandle {
    producer: Producer<Message>,
}

impl MidiHandle {
    /// Sends a message from the MIDI thread. Returns `false` if the queue is full.
    pub fn send(&mut self, msg: Message) -> bool {
        self.producer.push(msg).is_ok()
    }

    /// Number of messages that can still be queued.
    #[must_use]
    pub fn free_slots(&self) -> usize {
        self.producer.slots()
    }
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

    /// Block peak meters of the last rendered block (lock-free, see [`Meters`]).
    #[must_use]
    pub fn meters(&self) -> Meters {
        self.shared.read_meters()
    }

    /// Takes the MIDI queue's producer (once). Returns `None` if it was already taken.
    pub fn take_midi_handle(&mut self) -> Option<MidiHandle> {
        self.midi.take().map(|producer| MidiHandle { producer })
    }

    /// A reader of the master-output spectrum ring (plugins milestone): the last
    /// [`crate::spectrum::SPECTRUM_SIZE`] mono output samples, lock-free. Cheap to clone.
    #[must_use]
    pub fn spectrum(&self) -> SpectrumReader {
        self.spectrum.clone()
    }

    /// Block peak of the reverb return (post return level) of the last rendered block,
    /// linear (plugins milestone; the UI applies its own decay).
    #[must_use]
    pub fn reverb_level(&self) -> f32 {
        self.shared.read_reverb_level()
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

/// Smoothed per-channel mixer gains, derived from a [`MixerState`] strip.
///
/// Five one-pole smoothers per channel: the fader gain (level times mute/solo, so a mute is a
/// 10 ms fade rather than a step), the two pan gains and the two send levels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct StripDsp {
    gain: Smoother,
    pan_l: Smoother,
    pan_r: Smoother,
    delay_send: Smoother,
    reverb_send: Smoother,
}

impl StripDsp {
    fn new(sample_rate: f32, mixer: &MixerState, channel: Channel) -> Self {
        let gain = mixer.effective_gain(channel);
        let (pl, pr) = mixer.pan_gains(channel);
        let strip = mixer.strip(channel);
        let s = |v: f32| Smoother::new(sample_rate, MIXER_SMOOTH_MS, v);
        Self {
            gain: s(gain),
            pan_l: s(pl),
            pan_r: s(pr),
            delay_send: s(strip.delay_send),
            reverb_send: s(strip.reverb_send),
        }
    }

    /// Points every smoother at the strip's current values (glides over ~10 ms).
    fn retarget(&mut self, mixer: &MixerState, channel: Channel) {
        let (pl, pr) = mixer.pan_gains(channel);
        let strip = mixer.strip(channel);
        self.gain.set_target(mixer.effective_gain(channel));
        self.pan_l.set_target(pl);
        self.pan_r.set_target(pr);
        self.delay_send.set_target(strip.delay_send);
        self.reverb_send.set_target(strip.reverb_send);
    }

    /// Jumps every smoother to the strip's current values (project load).
    fn snap(&mut self, mixer: &MixerState, channel: Channel) {
        self.retarget(mixer, channel);
        for s in [
            &mut self.gain,
            &mut self.pan_l,
            &mut self.pan_r,
            &mut self.delay_send,
            &mut self.reverb_send,
        ] {
            s.snap(s.target());
        }
    }
}

/// Stereo accumulators of one mixer frame.
#[derive(Debug, Clone, Copy, Default)]
struct Buses {
    dry: (f32, f32),
    delay: (f32, f32),
    reverb: (f32, f32),
}

/// Audio-side engine. Owns all DSP state. Lives on the audio thread (or in a test).
pub struct Engine {
    sample_rate: f32,
    consumer: Consumer<Message>,
    /// Messages from the MIDI input thread (second SPSC queue).
    midi_consumer: Consumer<Message>,
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
    /// All patterns, the chain, the mode and the selected pattern.
    bank: Bank,
    /// Index of the pattern that step/row/volume/tune edits target.
    edit: u8,
    /// Index of the pattern the scheduler is playing (or will play on `play`).
    playing: u8,
    /// 0-based bar within the playing pattern.
    bar: u8,
    /// Chain entry being played (song mode).
    song_index: u8,
    /// Completed plays of the current chain entry.
    song_repeat: u8,
    /// Pattern queued for the next bar boundary (pattern mode).
    queued: Option<u8>,
    /// Set when song mode was entered while playing: restart the chain at the next bar.
    song_restart: bool,
    /// The procedural drum kit; its rows feed the first five mixer channels.
    drums: DrumKit,
    /// Mixer settings (the authoritative, unsmoothed values).
    mixer: MixerState,
    /// Smoothed per-channel gains derived from `mixer`.
    strips: [StripDsp; Channel::COUNT],
    /// Block peaks of the last `render`.
    meters: Meters,
    /// Playhead and meters published to the UI.
    shared: Arc<Shared>,
    /// Cached `transport.samples_per_step(sample_rate)` (f64 so 16 steps stay exact).
    samples_per_step: f64,
    /// Cached bar length in whole samples (`round(16 * samples_per_step)`).
    bar_len: u64,
    /// Samples elapsed since the start of the current bar (`0..bar_len`); the sample about to
    /// be rendered.
    pos: u64,
    /// Next step of the bar to fire (`0..=16`; 16 means every step has fired and the bar is
    /// waiting to wrap).
    next_step: usize,
    /// Monotonic sample clock used to time the sequencer note-offs.
    clock: u64,
    /// The synth notes the sequencer is currently sounding: `(pitch, clock value at which
    /// it is released)`, at most one entry per pitch, one slot per voice.
    seq_active: [Option<(u8, u64)>; MAX_VOICES],
    /// Earliest `off` among `seq_active` (`u64::MAX` when none), so the per-sample tick only
    /// scans the slots when a release is due.
    seq_next_off: u64,
    /// Notes currently held from the keyboard (`note_on` without `note_off`); the sequencer
    /// never releases these.
    kbd_held: [bool; 128],
    /// Master parametric EQ (plugins milestone): after the effect returns, before the master
    /// gain and the limiter.
    eq: ParametricEq,
    /// Every reverb value (plugins milestone); size / damping / mix mirror `params`.
    reverb_state: ReverbState,
    /// Master-output spectrum ring writer (plugins milestone).
    spectrum: SpectrumTap,
    /// Block peak of the reverb return.
    reverb_peak: f32,
}

/// Creates an engine and its UI-side handle.
///
/// All buffers and DSP state are allocated here, once; the returned [`Engine`] does not
/// allocate afterwards. `sample_rate` is in Hz (tests use 48 000). The bank starts empty
/// (every pattern blank, chain `[A x1]`, pattern mode).
pub fn new_engine(sample_rate: f32) -> (EngineHandle, Engine) {
    // A non-finite or non-positive rate would make the step length 0 and fire every step on
    // every sample; fall back to 48 kHz like the kit and the FX chain do.
    let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        48_000.0
    };
    let (producer, consumer) = RingBuffer::new(QUEUE_CAPACITY);
    let (midi_producer, midi_consumer) = RingBuffer::new(QUEUE_CAPACITY);
    let shared = Arc::new(Shared::default());
    let mut params = [0.0_f32; ParamId::COUNT];
    for id in ParamId::ALL {
        params[id.index()] = id.default_value();
    }
    let transport = Transport::default();
    let samples_per_step = transport.samples_per_step(sample_rate);
    let bar_len = bar_len_samples(samples_per_step);
    let mixer = MixerState::default();
    let strips = Channel::ALL.map(|ch| StripDsp::new(sample_rate, &mixer, ch));
    let (spectrum_tap, spectrum_reader) = spectrum_pair();
    (
        EngineHandle {
            producer,
            shared: Arc::clone(&shared),
            midi: Some(midi_producer),
            spectrum: spectrum_reader,
        },
        Engine {
            sample_rate,
            consumer,
            midi_consumer,
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
            transport,
            bank: Bank::empty(),
            edit: 0,
            playing: 0,
            bar: 0,
            song_index: 0,
            song_repeat: 0,
            queued: None,
            song_restart: false,
            drums: DrumKit::new(sample_rate),
            mixer,
            strips,
            meters: Meters::SILENT,
            shared,
            samples_per_step,
            bar_len,
            pos: 0,
            next_step: 0,
            clock: 0,
            seq_active: [None; MAX_VOICES],
            seq_next_off: u64::MAX,
            kbd_held: [false; 128],
            eq: ParametricEq::new(sample_rate),
            reverb_state: ReverbState::default(),
            spectrum: spectrum_tap,
            reverb_peak: 0.0,
        },
    )
}

/// Whole-sample bar length for a step length: `round(16 * samples_per_step)`, at least 1.
fn bar_len_samples(samples_per_step: f64) -> u64 {
    ((samples_per_step * STEPS as f64).round() as u64).max(1)
}

/// Wraps a pattern index into `0..PATTERN_COUNT`.
fn pattern_index(index: u8) -> usize {
    usize::from(index) % PATTERN_COUNT
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
        self.kbd_held[usize::from(note.min(127))] = true;
        self.voices.note_on(note, velocity);
    }

    /// Releases every voice playing MIDI `note`; it fades out over the release time.
    pub fn note_off(&mut self, note: u8) {
        self.kbd_held[usize::from(note.min(127))] = false;
        self.voices.note_off(note);
    }

    /// Releases every sounding voice (keyboard and sequencer notes alike).
    pub fn all_notes_off(&mut self) {
        self.kbd_held = [false; 128];
        self.seq_active = [None; MAX_VOICES];
        self.seq_next_off = u64::MAX;
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
            // The reverb's size / damping / mix are shared with the plugin's `ReverbState`.
            ParamId::ReverbSize => {
                self.reverb_state.size = value;
                self.fx.set_param(id, value);
            }
            ParamId::ReverbDamping => {
                self.reverb_state.damping = value;
                self.fx.set_param(id, value);
            }
            ParamId::ReverbMix => {
                self.reverb_state.mix = value;
                self.fx.set_param(id, value);
            }
            id if id.is_fx() => self.fx.set_param(id, value),
            _ => {}
        }
    }

    // --- plugins: master EQ and reverb (plugins milestone) --------------------------------

    /// The master EQ's target state (what the UI mirrors).
    #[must_use]
    pub fn eq(&self) -> &EqState {
        self.eq.state()
    }

    /// Sets one value of EQ band `band` (wraps into `0..BAND_COUNT`); clamped and smoothed.
    /// Returns the stored value.
    pub fn set_eq(&mut self, band: u8, param: EqParam, value: f32) -> f32 {
        self.eq.set(usize::from(band), param, value)
    }

    /// Switches the master EQ on or off (faded).
    pub fn set_eq_on(&mut self, on: bool) {
        self.eq.set_on(on);
    }

    /// Sets the master EQ output gain in dB (clamped, smoothed). Returns the stored value.
    pub fn set_eq_gain(&mut self, gain_db: f32) -> f32 {
        self.eq.set_gain_db(gain_db)
    }

    /// Replaces the whole EQ (project load): values are normalised and the smoothers snap.
    pub fn set_eq_state(&mut self, state: &EqState) {
        self.eq.set_state(state);
    }

    /// Every reverb value (size / damping / mix agree with the `ParamId` view).
    #[must_use]
    pub fn reverb(&self) -> &ReverbState {
        &self.reverb_state
    }

    /// Sets one reverb value (clamped). Size, damping and mix also update the matching
    /// `ParamId` so both views agree; the rest go straight to the reverb. Returns the stored
    /// value.
    pub fn set_reverb(&mut self, param: ReverbParam, value: f32) -> f32 {
        let v = self.reverb_state.set(param, value);
        match param {
            ReverbParam::Size => self.set_param(ParamId::ReverbSize, v),
            ReverbParam::Damping => self.set_param(ParamId::ReverbDamping, v),
            ReverbParam::Mix => self.set_param(ParamId::ReverbMix, v),
            ReverbParam::PreDelay => self.fx.reverb_mut().set_pre_delay_ms(v),
            ReverbParam::Width => self.fx.reverb_mut().set_width(v),
            ReverbParam::LowCut => self.fx.reverb_mut().set_low_cut_hz(v),
            ReverbParam::HighCut => self.fx.reverb_mut().set_high_cut_hz(v),
        }
        // `set_param` re-clamps with the ParamId range (identical to the state's).
        self.reverb_state.get(param)
    }

    /// Replaces every reverb value (project load).
    pub fn set_reverb_state(&mut self, state: &ReverbState) {
        for p in ReverbParam::ALL {
            self.set_reverb(p, state.get(p));
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
            // Keep the fractional position within the bar so a tempo change while playing
            // neither jumps nor repeats/skips steps (`next_step` is left alone: the steps
            // already fired this bar stay fired).
            let old_bar = self.bar_len;
            self.samples_per_step = self.transport.samples_per_step(self.sample_rate);
            self.bar_len = bar_len_samples(self.samples_per_step);
            let frac = self.pos as f64 / old_bar as f64;
            self.pos = ((frac * self.bar_len as f64).round() as u64).min(self.bar_len - 1);
        }
    }

    /// Current tempo in BPM.
    #[must_use]
    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    /// Starts (or restarts) the sequencer from step 0 of the current pattern (song mode:
    /// chain entry 0); step 0 fires on the next rendered sample.
    pub fn play(&mut self) {
        self.release_seq_notes();
        self.pos = 0;
        self.next_step = 0;
        self.rewind();
        self.transport.set_playing(true);
        self.publish();
    }

    /// Stops the sequencer, releases its notes and rewinds to step 0. Drums ring out
    /// naturally and keyboard notes keep sounding.
    pub fn stop(&mut self) {
        self.transport.set_playing(false);
        self.release_seq_notes();
        self.pos = 0;
        self.next_step = 0;
        self.rewind();
        self.publish();
    }

    /// Sets the swing amount (`0.0..=`[`MAX_SWING`](crate::transport::MAX_SWING)).
    pub fn set_swing(&mut self, swing: f32) {
        self.transport.set_swing(swing);
    }

    /// The drum kit (read-only; volume and tune go through
    /// [`set_drum_volume`](Self::set_drum_volume) / [`set_drum_tune`](Self::set_drum_tune)).
    #[must_use]
    pub fn drums(&self) -> &DrumKit {
        &self.drums
    }

    // --- mixer ----------------------------------------------------------------------------

    /// The mixer settings (level, pan, mute, solo, sends per channel).
    #[must_use]
    pub fn mixer(&self) -> &MixerState {
        &self.mixer
    }

    /// Sets one mixer value (clamped with [`ChannelParam::clamp`]; mute/solo snap to 0/1).
    /// The audible gains glide over about [`MIXER_SMOOTH_MS`] so toggles never click. A solo
    /// change re-targets every channel. Returns the stored value.
    pub fn set_mixer(&mut self, channel: Channel, param: ChannelParam, value: f32) -> f32 {
        let stored = self.mixer.set(channel, param, value);
        if param == ChannelParam::Solo {
            for ch in Channel::ALL {
                self.strips[ch.index()].retarget(&self.mixer, ch);
            }
        } else {
            self.strips[channel.index()].retarget(&self.mixer, channel);
        }
        stored
    }

    /// Replaces the whole mixer (project load). Values are normalised into range and the
    /// smoothers jump straight to them (no glide), so an exported or freshly loaded project
    /// starts with its exact mix. Meant to be called before the stream starts; on the audio
    /// thread it is a fixed-size copy, not an allocation.
    pub fn set_mixer_state(&mut self, mixer: &MixerState) {
        self.mixer = *mixer;
        self.mixer.normalise();
        for ch in Channel::ALL {
            self.strips[ch.index()].snap(&self.mixer, ch);
        }
    }

    /// Block peaks of the last `render` call (the UI reads the same data through
    /// [`EngineHandle::meters`]).
    #[must_use]
    pub fn meters(&self) -> Meters {
        self.meters
    }

    /// The sequencer playhead as the audio side sees it right now (the UI reads the same
    /// data through [`EngineHandle::position`]).
    #[must_use]
    pub fn position(&self) -> Position {
        let (step, frac) = self.step_and_frac();
        Position {
            step,
            frac,
            playing: self.transport.is_playing(),
            pattern: self.playing,
            bar: self.bar,
            song_index: self.song_index,
            queued: self.queued,
        }
    }

    /// The transport (bpm, swing, playing).
    #[must_use]
    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    // --- bank -----------------------------------------------------------------------------

    /// All patterns, the chain, the play mode and the selected pattern.
    #[must_use]
    pub fn bank(&self) -> &Bank {
        &self.bank
    }

    /// Replaces the whole bank (project load). Copies and normalises `bank`, makes its
    /// `current` pattern the edit pattern and rewinds to the start (restarting playback if
    /// the transport is running). Meant to be called before the stream starts; on the audio
    /// thread it is a fixed-size copy, not an allocation.
    pub fn set_bank(&mut self, bank: &Bank) {
        self.bank = bank.clone();
        self.bank.normalise();
        self.edit = self.bank.current;
        if self.transport.is_playing() {
            self.play();
        } else {
            self.rewind();
            self.publish();
        }
    }

    /// Index of the pattern that step/row/volume/tune edits target.
    #[must_use]
    pub fn edit_pattern(&self) -> u8 {
        self.edit
    }

    /// Chooses the edit pattern (`0..PATTERN_COUNT`, wraps). Does not change what plays.
    pub fn set_edit_pattern(&mut self, index: u8) {
        self.edit = pattern_index(index) as u8;
    }

    /// Index of the pattern the scheduler is playing (or will play on `play`).
    #[must_use]
    pub fn playing_pattern(&self) -> u8 {
        self.playing
    }

    /// The pattern queued for the next bar boundary, if any (pattern mode).
    #[must_use]
    pub fn queued_pattern(&self) -> Option<u8> {
        self.queued
    }

    /// Selects pattern `index`: it becomes the edit pattern and the bank's `current`. In
    /// pattern mode (or song mode with an empty chain) while playing the switch of the
    /// *playing* pattern is queued to the next 16-step bar boundary; selecting the pattern
    /// that already plays cancels a pending queue. When stopped the switch is immediate.
    pub fn select_pattern(&mut self, index: u8) {
        let index = pattern_index(index) as u8;
        self.edit = index;
        self.bank.current = index;
        if !self.transport.is_playing() {
            self.rewind();
        } else if !self.song_active() {
            self.queued = (index != self.playing).then_some(index);
        }
    }

    /// Switches between pattern and song mode. When stopped the start position follows
    /// (song: chain entry 0). While playing, song mode restarts the chain from entry 0 at the
    /// next bar boundary; pattern mode queues the selected pattern (if it is not already
    /// playing) for the next bar boundary.
    pub fn set_play_mode(&mut self, mode: PlayMode) {
        self.bank.mode = mode;
        if !self.transport.is_playing() {
            self.rewind();
            return;
        }
        match mode {
            PlayMode::Song => {
                self.queued = None;
                self.song_restart = self.bank.chain_len > 0;
            }
            PlayMode::Pattern => {
                self.song_restart = false;
                self.queued = (self.bank.current != self.playing).then_some(self.bank.current);
            }
        }
    }

    /// Current play mode.
    #[must_use]
    pub fn play_mode(&self) -> PlayMode {
        self.bank.mode
    }

    /// Sets the length of pattern `index` in steps (16/32/64; anything else rounds up to the
    /// next allowed length). For the playing pattern it applies at its next bar boundary.
    pub fn set_pattern_length(&mut self, index: u8, steps: u8) {
        self.bank.pattern_mut(index).set_length_steps(steps);
    }

    /// Copies pattern `from` over pattern `to`. Kit volume/tune follow if `to` is playing.
    pub fn copy_pattern(&mut self, from: u8, to: u8) {
        self.bank.copy_pattern(from, to);
        if pattern_index(to) == pattern_index(self.playing) {
            self.apply_kit_settings(self.playing);
        }
    }

    /// Clears every step of pattern `index` (length, volume and tune are kept).
    pub fn clear_pattern(&mut self, index: u8) {
        self.bank.clear_pattern(index);
    }

    // --- notes (M7) -----------------------------------------------------------------------

    /// Sets or adds a synth note of pattern `pattern`: `index < note_count` replaces that
    /// note, `index == note_count` appends. Returns the note's index in the (re-sorted) list,
    /// or `None` when `index` is out of range or the note is invalid (`length == 0`, or
    /// `start` at or beyond the pattern length); nothing changes then. The grid lane is
    /// rebuilt. Notes already sounding are unaffected.
    pub fn set_note(&mut self, pattern: u8, index: u8, note: Note) -> Option<usize> {
        let p = self.bank.pattern_mut(pattern);
        let index = usize::from(index);
        match index.cmp(&p.notes().len()) {
            std::cmp::Ordering::Less => p.set_note(index, note),
            std::cmp::Ordering::Equal => p.add_note(note),
            std::cmp::Ordering::Greater => None,
        }
    }

    /// Removes synth note `index` of pattern `pattern`. Returns `false` when `index` is out
    /// of range. The grid lane is rebuilt.
    pub fn remove_note(&mut self, pattern: u8, index: u8) -> bool {
        self.bank
            .pattern_mut(pattern)
            .remove_note(usize::from(index))
    }

    /// Removes every synth note of pattern `pattern` (the grid lane becomes rests).
    pub fn clear_notes(&mut self, pattern: u8) {
        self.bank.pattern_mut(pattern).clear_notes();
    }

    /// Sets chain entry `index` (`0..MAX_CHAIN`), growing the chain to include it. Takes
    /// effect when the song next reaches that entry.
    pub fn set_chain_entry(&mut self, index: u8, pattern: u8, repeats: u8) {
        self.bank
            .set_entry(index, ChainEntry::new(pattern, repeats));
        if !self.transport.is_playing() {
            self.rewind();
        }
    }

    /// Sets the number of valid chain entries (`0..=MAX_CHAIN`; entries beyond the current
    /// length are reset to `A x1`).
    pub fn set_chain_len(&mut self, len: u8) {
        let len = usize::from(len).min(MAX_CHAIN);
        let old = usize::from(self.bank.chain_len).min(MAX_CHAIN);
        for e in &mut self.bank.chain[old.min(len)..len] {
            *e = ChainEntry::default();
        }
        self.bank.chain_len = len as u8;
        if !self.transport.is_playing() {
            self.rewind();
        }
    }

    // --- edit pattern ---------------------------------------------------------------------

    /// The edit pattern (what the step/row/volume/tune setters change).
    #[must_use]
    pub fn pattern(&self) -> &Pattern {
        self.bank.pattern(self.edit)
    }

    /// Replaces the edit pattern (call before the stream starts or accept a one-off
    /// fixed-size copy on the audio thread). Kit volume/tune follow if it is playing.
    pub fn set_pattern(&mut self, pattern: &Pattern) {
        let slot = self.bank.pattern_mut(self.edit);
        *slot = pattern.clone();
        slot.normalise();
        if self.edit_is_playing() {
            self.apply_kit_settings(self.playing);
        }
    }

    /// Sets one drum step of the edit pattern.
    pub fn set_drum_step(&mut self, row: DrumRow, step: u8, on: bool, accent: bool) {
        self.edit_mut().set_drum_step(row, step, on, accent);
    }

    /// Sets one synth-lane step of the edit pattern.
    pub fn set_synth_step(&mut self, step: u8, note: Option<u8>, accent: bool) {
        self.edit_mut().set_synth_step(step, note, accent);
    }

    /// Per-row drum volume in dB of the edit pattern; applied to the kit when that pattern
    /// is the playing one.
    pub fn set_drum_volume(&mut self, row: DrumRow, db: f32) {
        self.edit_mut().set_drum_volume(row, db);
        if self.edit_is_playing() {
            let db = self.pattern().drum_volume[row.index()];
            self.drums.set_volume(row, db);
        }
    }

    /// Per-row drum tune in semitones of the edit pattern; applied to the kit when that
    /// pattern is the playing one.
    pub fn set_drum_tune(&mut self, row: DrumRow, semitones: f32) {
        self.edit_mut().set_drum_tune(row, semitones);
        if self.edit_is_playing() {
            let st = self.pattern().drum_tune[row.index()];
            self.drums.set_tune(row, st);
        }
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

    /// `true` while the engine would produce non-silent output or is about to: any voice or
    /// drum sounding, the tone on or fading out, or the sequencer running.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.tone.is_active()
            || self.voices.is_active()
            || self.drums.is_active()
            || self.transport.is_playing()
    }

    // --- internals ------------------------------------------------------------------------

    fn edit_mut(&mut self) -> &mut Pattern {
        self.bank.pattern_mut(self.edit)
    }

    fn edit_is_playing(&self) -> bool {
        pattern_index(self.edit) == pattern_index(self.playing)
    }

    /// `true` when the chain drives playback: song mode with at least one entry.
    fn song_active(&self) -> bool {
        self.bank.mode == PlayMode::Song && self.bank.chain_len > 0
    }

    /// Pattern of chain entry `index` (wrapped into the valid chain).
    fn chain_pattern(&self, index: u8) -> u8 {
        let len = usize::from(self.bank.chain_len).clamp(1, MAX_CHAIN);
        self.bank.chain[usize::from(index) % len].pattern
    }

    /// Number of 16-step bars of pattern `index`.
    fn bars_of(&self, index: u8) -> u8 {
        self.bank.pattern(index).bars().min(u8::MAX as usize) as u8
    }

    /// Rewinds to the start position: bar 0, no queue, song entry 0, playing the pattern
    /// that `play` would start with. Does not touch `pos`/`next_step` or the transport.
    fn rewind(&mut self) {
        self.bar = 0;
        self.queued = None;
        self.song_restart = false;
        self.song_index = 0;
        self.song_repeat = 0;
        let start = if self.song_active() {
            self.chain_pattern(0)
        } else {
            self.bank.current
        };
        self.switch_to(start);
    }

    /// Makes `index` the playing pattern at bar 0, releases every active sequencer note (a
    /// note cannot outlive its pattern's pass) and pushes the pattern's volume/tune into the
    /// kit.
    fn switch_to(&mut self, index: u8) {
        self.release_seq_notes();
        self.playing = pattern_index(index) as u8;
        self.bar = 0;
        self.apply_kit_settings(self.playing);
    }

    /// Pushes pattern `index`'s per-row volume and tune into the drum kit (smoothed there).
    fn apply_kit_settings(&mut self, index: u8) {
        let p = self.bank.pattern(index);
        let (volume, tune) = (p.drum_volume, p.drum_tune);
        for row in DrumRow::ALL {
            self.drums.set_volume(row, volume[row.index()]);
            self.drums.set_tune(row, tune[row.index()]);
        }
    }

    /// Runs when `pos` wraps: advances the bar and applies pattern switches, pattern
    /// restarts and chain progression (see the module docs).
    fn bar_boundary(&mut self) {
        self.bar = self.bar.saturating_add(1);
        if self.song_active() {
            self.queued = None;
            if self.song_restart {
                self.song_restart = false;
                self.song_index = 0;
                self.song_repeat = 0;
                let start = self.chain_pattern(0);
                self.switch_to(start);
            } else if self.bar >= self.bars_of(self.playing) {
                let len = usize::from(self.bank.chain_len).clamp(1, MAX_CHAIN);
                let entry = self.bank.chain[usize::from(self.song_index) % len];
                self.song_repeat = self.song_repeat.saturating_add(1);
                if self.song_repeat >= entry.repeats.max(1) {
                    self.song_repeat = 0;
                    self.song_index = ((usize::from(self.song_index) + 1) % len) as u8;
                }
                let next = self.chain_pattern(self.song_index);
                self.switch_to(next);
            }
        } else {
            self.song_restart = false;
            if let Some(next) = self.queued.take() {
                self.bank.current = next;
                self.switch_to(next);
            } else if self.bar >= self.bars_of(self.playing) {
                let current = self.bank.current;
                self.switch_to(current);
            }
        }
    }

    /// Publishes the current position to the UI.
    fn publish(&self) {
        self.shared.publish(self.position());
    }

    /// Current step and the fraction of it elapsed, derived from `pos` on the unswung grid.
    fn step_and_frac(&self) -> (u8, f32) {
        let steps = self.pos as f64 / self.samples_per_step;
        let step = (steps.floor() as usize).min(STEPS - 1);
        let frac = (steps - step as f64).clamp(0.0, 0.999) as f32;
        (step as u8, frac)
    }

    /// Releases a sequencer pitch, unless the keyboard holds the same note.
    fn release_pitch(&mut self, pitch: u8) {
        if !self.kbd_held[usize::from(pitch.min(127))] {
            self.voices.note_off(pitch);
        }
    }

    /// Releases every active sequencer note (keyboard-held pitches excepted).
    fn release_seq_notes(&mut self) {
        for i in 0..MAX_VOICES {
            if let Some((pitch, _)) = self.seq_active[i].take() {
                self.release_pitch(pitch);
            }
        }
        self.seq_next_off = u64::MAX;
    }

    /// Recomputes `seq_next_off` from the active slots.
    fn refresh_next_off(&mut self) {
        self.seq_next_off = self
            .seq_active
            .iter()
            .flatten()
            .map(|&(_, off)| off)
            .min()
            .unwrap_or(u64::MAX);
    }

    /// Clock value at which pattern step `step` (`bar * 16 + step`, possibly in a later bar)
    /// of the playing pattern triggers, given that the current bar started `pos` samples ago;
    /// steps past the pattern end map to the pattern's wrap (the first sample of the next
    /// pass). The wrap is never before the end of the bar being played: a pattern shortened
    /// below the current bar finishes that bar first (M5 rule), and so do its notes.
    fn step_clock(&self, step: usize) -> u64 {
        let end_bars = usize::from(self.bars_of(self.playing)).max(usize::from(self.bar) + 1);
        let step = step.min(end_bars * STEPS);
        let bar_start = self.clock.saturating_sub(self.pos);
        let bars_ahead = (step / STEPS).saturating_sub(usize::from(self.bar)) as u64;
        bar_start
            + bars_ahead * self.bar_len
            + self
                .transport
                .step_trigger_sample(step % STEPS, self.samples_per_step)
    }

    /// Starts `note` (which begins on the step being fired) and schedules its release for
    /// one sample before the trigger of step `start + length` (the pattern's wrap at the
    /// latest). A pitch that is already sounding from the sequencer is released and its slot
    /// reused; when every slot is taken the note ending soonest is cut.
    fn trigger_note(&mut self, note: Note) {
        let pitch = note.pitch.min(127);
        let off = self
            .step_clock(note.end())
            .saturating_sub(1)
            .max(self.clock);
        let slot = self
            .seq_active
            .iter()
            .position(|s| s.is_some_and(|(p, _)| p == pitch))
            .or_else(|| self.seq_active.iter().position(Option::is_none))
            .or_else(|| {
                self.seq_active
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| s.map(|(_, off)| (i, off)))
                    .min_by_key(|&(_, off)| off)
                    .map(|(i, _)| i)
            })
            .unwrap_or(0);
        if let Some((old, _)) = self.seq_active[slot].take() {
            self.release_pitch(old);
        }
        self.voices.note_on(pitch, note.velocity);
        self.seq_active[slot] = Some((pitch, off));
        self.seq_next_off = self.seq_next_off.min(off);
    }

    /// Fires every drum and synth event of step `step` of the current bar of the playing
    /// pattern: drum hits, then every note whose `start` is that pattern step.
    fn fire_step(&mut self, step: usize) {
        let index = (usize::from(self.bar) * STEPS + step).min(MAX_STEPS - 1);
        let pattern = self.bank.pattern(self.playing);
        let mut drums = [DrumStep::default(); DrumRow::COUNT];
        for row in DrumRow::ALL {
            drums[row.index()] = pattern.drums[row.index()][index];
        }
        let note_count = pattern.notes().len();
        for row in DrumRow::ALL {
            if let Some(velocity) = drums[row.index()].velocity() {
                self.drums.trigger(row, velocity);
            }
        }
        // Notes are sorted by start, so the matching ones are contiguous; the scan over at
        // most MAX_NOTES fixed slots is cheap and allocation-free.
        for i in 0..note_count {
            let note = self.bank.pattern(self.playing).notes[i];
            if usize::from(note.start) == index {
                self.trigger_note(note);
            } else if usize::from(note.start) > index {
                break;
            }
        }
    }

    /// Releases every sequencer note whose off sample has come.
    fn release_due_notes(&mut self) {
        for i in 0..MAX_VOICES {
            if let Some((pitch, off)) = self.seq_active[i]
                && self.clock >= off
            {
                self.seq_active[i] = None;
                self.release_pitch(pitch);
            }
        }
        self.refresh_next_off();
    }

    /// One sample of the scheduler: fires due steps, releases sequencer notes whose off
    /// sample has come and advances the bar position (wrapping at the bar end).
    #[inline]
    fn sequencer_tick(&mut self) {
        if self.transport.is_playing() {
            while self.next_step < STEPS
                && self
                    .transport
                    .step_trigger_sample(self.next_step, self.samples_per_step)
                    <= self.pos
            {
                let step = self.next_step;
                self.next_step += 1;
                self.fire_step(step);
            }
        }
        if self.clock >= self.seq_next_off {
            self.release_due_notes();
        }
        self.clock += 1;
        if self.transport.is_playing() {
            self.pos += 1;
            if self.pos >= self.bar_len {
                self.pos = 0;
                self.next_step = 0;
                self.bar_boundary();
            }
        }
    }

    /// Applies all pending messages from the UI. Called at the start of every `render`.
    pub fn process_messages(&mut self) {
        while let Ok(msg) = self.consumer.pop() {
            self.apply_message(msg);
        }
        while let Ok(msg) = self.midi_consumer.pop() {
            self.apply_message(msg);
        }
    }

    /// Applies one message (shared by both queues).
    fn apply_message(&mut self, msg: Message) {
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
            } => self.set_drum_step(row, step, on, accent),
            Message::SetSynthStep { step, note, accent } => self.set_synth_step(step, note, accent),
            Message::ClearRow(row) => self.edit_mut().clear_row(row),
            Message::ClearSynth => self.edit_mut().clear_synth(),
            Message::ClearAll => self.edit_mut().clear_all(),
            Message::SetDrumVolume { row, db } => self.set_drum_volume(row, db),
            Message::SetDrumTune { row, semitones } => self.set_drum_tune(row, semitones),
            Message::SetEditPattern(index) => self.set_edit_pattern(index),
            Message::SetPatternLength { pattern, steps } => self.set_pattern_length(pattern, steps),
            Message::CopyPattern { from, to } => self.copy_pattern(from, to),
            Message::ClearPattern(index) => self.clear_pattern(index),
            Message::SelectPattern(index) => self.select_pattern(index),
            Message::SetPlayMode(mode) => self.set_play_mode(mode),
            Message::SetChainEntry {
                index,
                pattern,
                repeats,
            } => self.set_chain_entry(index, pattern, repeats),
            Message::SetChainLen(len) => self.set_chain_len(len),
            Message::SetNote {
                pattern,
                index,
                note,
            } => {
                self.set_note(pattern, index, note);
            }
            Message::RemoveNote { pattern, index } => {
                self.remove_note(pattern, index);
            }
            Message::ClearNotes(pattern) => self.clear_notes(pattern),
            Message::SetMixer {
                channel,
                param,
                value,
            } => {
                self.set_mixer(channel, param, value);
            }
            Message::SetEq { band, param, value } => {
                self.set_eq(band, param, value);
            }
            Message::SetEqOn(on) => self.set_eq_on(on),
            Message::SetEqGain(db) => {
                self.set_eq_gain(db);
            }
            Message::SetReverb { param, value } => {
                self.set_reverb(param, value);
            }
        }
    }

    /// One frame of the mixer: scales each channel input by its smoothed gain, pans it into
    /// the dry bus, adds its post-fader sends to the delay and reverb buses and tracks the
    /// per-channel block peak (post-fader, pre-pan).
    #[inline]
    fn mix_frame(&mut self, inputs: &[f32; Channel::COUNT]) -> Buses {
        let mut buses = Buses::default();
        for (i, strip) in self.strips.iter_mut().enumerate() {
            let x = inputs[i];
            let post = if x.is_finite() {
                x * strip.gain.tick()
            } else {
                0.0
            };
            let (pl, pr) = (strip.pan_l.tick(), strip.pan_r.tick());
            let (l, r) = (post * pl, post * pr);
            buses.dry.0 += l;
            buses.dry.1 += r;
            let ds = strip.delay_send.tick();
            buses.delay.0 += l * ds;
            buses.delay.1 += r * ds;
            let rs = strip.reverb_send.tick();
            buses.reverb.0 += l * rs;
            buses.reverb.1 += r * rs;
            let a = post.abs();
            if a > self.meters.channel[i] {
                self.meters.channel[i] = a;
            }
        }
        buses
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
        self.meters = Meters::SILENT;
        self.reverb_peak = 0.0;
        let mut rows = [0.0_f32; DrumRow::COUNT];
        let mut inputs = [0.0_f32; Channel::COUNT];
        for [l, r] in stereo_frames {
            self.sequencer_tick();
            // Five drum rows plus the synth (through its drive insert) feed the mixer; the
            // diagnostic test tone bypasses mixer, effects and the master EQ.
            self.drums.tick_rows(&mut rows);
            inputs[..DrumRow::COUNT].copy_from_slice(&rows);
            inputs[Channel::Synth.index()] = self.fx.drive().process(self.voices.tick());
            let buses = self.mix_frame(&inputs);
            let (fl, fr) = self.fx.process_bus(buses.dry, buses.delay, buses.reverb);
            let (wl, wr) = self.fx.last_reverb_wet();
            let wet = wl.abs().max(wr.abs());
            if wet > self.reverb_peak {
                self.reverb_peak = wet;
            }
            let (fl, fr) = self.eq.process(fl, fr);
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
            let (al, ar) = (l.abs(), r.abs());
            if al > self.meters.master[0] {
                self.meters.master[0] = al;
            }
            if ar > self.meters.master[1] {
                self.meters.master[1] = ar;
            }
            self.spectrum.push((*l + *r) * 0.5);
        }
        tail.fill(0.0);
        self.spectrum.publish();
        self.shared.publish_meters(&self.meters);
        self.shared.publish_reverb_level(self.reverb_peak);
        self.publish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_word_packs_and_unpacks() {
        for (p, b, s, q) in [
            (0, 0, 0, None),
            (7, 3, 63, Some(5)),
            (1, 1, 200, Some(0)),
            (255, 255, 255, Some(254)),
        ] {
            assert_eq!(unpack_song(pack_song(p, b, s, q)), (p, b, s, q));
        }
        let shared = Shared::default();
        let pos = Position {
            step: 9,
            frac: 0.25,
            playing: true,
            pattern: 2,
            bar: 1,
            song_index: 3,
            queued: Some(4),
        };
        shared.publish(pos);
        assert_eq!(shared.read(), pos);
        assert_eq!(pos.pattern_step(), 25);
        assert_eq!(Position::default(), Position::stopped(0));
    }
}
