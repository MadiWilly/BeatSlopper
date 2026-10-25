//! Built-in sound presets: plain `(ParamId, value)` data so the UI can apply them through
//! `Message::SetParam` as well.
//!
//! [`Preset::apply`] first resets **every** parameter to its default (master gain and limiter
//! included), then applies the preset's values; a preset therefore fully describes a sound
//! and parameters it does not list are at their defaults.
//!
//! Two lists live here (M9):
//! - [`ALL`]: the original eight M2 presets, unchanged in name and values (the synth panel's
//!   preset row still indexes it).
//! - [`LIBRARY`]: the preset library, grouped by [`Category`] (at least ten per category),
//!   browsed with [`by_category`], [`search`], [`find`] and [`index_of`]. Every entry was
//!   designed by DSP reasoning (the build machine has no speakers) and is held to measurable
//!   properties by `tests/presets.rs`: in range, renders finite and non-silent, peak at most
//!   1.0 after the limiter, and decays within 8 s of release unless tagged `"drone"`
//!   ([`Preset::is_drone`]). Presets with vinyl noise ([`Preset::has_vinyl_noise`]) decay to
//!   their noise bed instead, because the lo-fi insert's hiss is not gated by the voices.

use crate::engine::Engine;
use crate::params::ParamId;
use crate::track::TrackId;

/// Preset browser category (M9). The order is the browser's tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    /// Dusty electric pianos, tape keys, music boxes.
    LofiKeys,
    /// Nylon, clean and muted plucked strings through tape and vinyl.
    LofiGuitars,
    /// Warm, wobbly, hissy pads.
    LofiPads,
    /// Supersaws, hoovers, square and FM leads.
    EdmLeads,
    /// 808s, subs, reeses, growls, acid.
    EdmBass,
    /// Short enveloped plucks, mallets and stabs.
    EdmPlucks,
    /// Big detuned pads, strings, gated pads.
    EdmPads,
    /// Risers, downlifters, impacts, echoes, textures.
    Risers,
}

impl Category {
    /// Every category in browser order.
    pub const ALL: [Category; 8] = [
        Category::LofiKeys,
        Category::LofiGuitars,
        Category::LofiPads,
        Category::EdmLeads,
        Category::EdmBass,
        Category::EdmPlucks,
        Category::EdmPads,
        Category::Risers,
    ];

    /// Display label for the category selector.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Category::LofiKeys => "Lofi Keys",
            Category::LofiGuitars => "Lofi Guitars",
            Category::LofiPads => "Lofi Pads",
            Category::EdmLeads => "EDM Leads",
            Category::EdmBass => "EDM Bass",
            Category::EdmPlucks => "EDM Plucks",
            Category::EdmPads => "EDM Pads",
            Category::Risers => "Risers & FX",
        }
    }

    /// Position in [`Category::ALL`] (the segmented control's index).
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The category at `index` in [`Category::ALL`] (wraps).
    #[must_use]
    pub const fn from_index(index: usize) -> Category {
        Category::ALL[index % Category::ALL.len()]
    }
}

/// A named set of parameter values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preset {
    /// Display name (unique within [`ALL`] and [`LIBRARY`] together).
    pub name: &'static str,
    /// Browser category.
    pub category: Category,
    /// Search tags: mood ("warm", "dark", "bright"), genre ("lofi", "edm", "trap", "house"),
    /// type ("ep", "piano", "guitar", "pad", "lead", "bass", "pluck", "riser", "fx", "drone").
    pub tags: &'static [&'static str],
    /// Parameter values; ids not listed are at their [`ParamId::default_value`].
    pub values: &'static [(ParamId, f32)],
}

impl Preset {
    /// Resets every parameter to its default (global ones included; the per-track ids land
    /// on the engine's edit track), then applies this preset's values.
    pub fn apply(&self, engine: &mut Engine) {
        for &id in ParamId::ALL {
            engine.set_param(id, id.default_value());
        }
        for &(id, value) in self.values {
            engine.set_param(id, value);
        }
    }

    /// Resets every per-track parameter of track `track` to its default, then applies this
    /// preset's per-track values there (M10). Global parameters are left alone.
    pub fn apply_to_track(&self, engine: &mut Engine, track: TrackId) {
        for &id in ParamId::ALL {
            if id.is_track() {
                engine.set_track_param(track, id, id.default_value());
            }
        }
        for &(id, value) in self.values {
            if id.is_track() {
                engine.set_track_param(track, id, value);
            }
        }
    }

    /// Value this preset gives `id` (its default when the preset does not list it).
    #[must_use]
    pub fn value(&self, id: ParamId) -> f32 {
        self.values
            .iter()
            .rev()
            .find(|(pid, _)| *pid == id)
            .map_or_else(|| id.default_value(), |(_, v)| id.clamp(*v))
    }

    /// `true` when the preset carries `tag` (ASCII case-insensitive).
    #[must_use]
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
    }

    /// `true` for presets tagged `"drone"`: their global delay/reverb settings keep the sound
    /// going for more than 8 s after release (self-oscillating echo, infinite hall, a vinyl
    /// bed), so the browser should not expect them to fall silent.
    #[must_use]
    pub fn is_drone(&self) -> bool {
        self.has_tag("drone")
    }

    /// `true` when the lo-fi insert adds a vinyl noise bed (`LofiNoise > 0` and `LofiMix >
    /// 0`): the track then hisses and crackles even while no note sounds.
    #[must_use]
    pub fn has_vinyl_noise(&self) -> bool {
        self.value(ParamId::LofiNoise) > 0.0 && self.value(ParamId::LofiMix) > 0.0
    }

    /// `true` when the preset lists `id` explicitly.
    #[must_use]
    pub fn lists(&self, id: ParamId) -> bool {
        self.values.iter().any(|(pid, _)| *pid == id)
    }
}

use ParamId as P;

/// `values![Osc1Wave: 1.0, Osc1Mix: 0.8]` -> `&[(P::Osc1Wave, 1.0), (P::Osc1Mix, 0.8)]`.
macro_rules! values {
    ($($id:ident : $v:expr),* $(,)?) => { &[$((P::$id, $v)),*] };
}

// --- the original eight (M2) ------------------------------------------------------------

/// Punchy mono-style bass: saw + square an octave-ish apart, closed filter, drive.
pub const BASS: Preset = Preset {
    name: "Bass",
    category: Category::EdmBass,
    tags: &["edm", "bass", "classic", "punchy"],
    values: &[
        (P::Osc1Wave, 1.0),
        (P::Osc1Detune, 0.0),
        (P::Osc1Mix, 0.8),
        (P::Osc2Wave, 2.0),
        (P::Osc2Detune, -6.0),
        (P::Osc2Mix, 0.4),
        (P::FilterCutoff, 420.0),
        (P::FilterResonance, 0.3),
        (P::FilterEnvAmount, 0.55),
        (P::AmpAttack, 0.003),
        (P::AmpDecay, 0.25),
        (P::AmpSustain, 0.5),
        (P::AmpRelease, 0.12),
        (P::LfoRate, 0.5),
        (P::LfoDepth, 0.0),
        (P::LfoTarget, 1.0),
        (P::Glide, 0.03),
        (P::DriveAmount, 0.5),
        (P::DriveMix, 0.6),
        (P::DelayMix, 0.0),
        (P::ReverbMix, 0.05),
    ],
};

/// Pure low sine with a touch of triangle: sub bass.
pub const SUB: Preset = Preset {
    name: "Sub",
    category: Category::EdmBass,
    tags: &["edm", "bass", "sub", "classic", "clean"],
    values: &[
        (P::Osc1Wave, 0.0),
        (P::Osc1Mix, 1.0),
        (P::Osc2Wave, 3.0),
        (P::Osc2Detune, 0.0),
        (P::Osc2Mix, 0.3),
        (P::FilterCutoff, 300.0),
        (P::FilterResonance, 0.0),
        (P::FilterEnvAmount, 0.2),
        (P::AmpAttack, 0.01),
        (P::AmpDecay, 0.3),
        (P::AmpSustain, 0.85),
        (P::AmpRelease, 0.2),
        (P::LfoDepth, 0.0),
        (P::Glide, 0.02),
        (P::DriveAmount, 0.1),
        (P::DriveMix, 0.3),
        (P::DelayMix, 0.0),
        (P::ReverbMix, 0.0),
    ],
};

/// Bright detuned saw lead with vibrato, a little glide and a synced delay.
pub const LEAD: Preset = Preset {
    name: "Lead",
    category: Category::EdmLeads,
    tags: &["edm", "lead", "classic", "bright", "saw"],
    values: &[
        (P::Osc1Wave, 1.0),
        (P::Osc1Detune, 8.0),
        (P::Osc1Mix, 0.7),
        (P::Osc2Wave, 1.0),
        (P::Osc2Detune, -8.0),
        (P::Osc2Mix, 0.7),
        (P::FilterCutoff, 3200.0),
        (P::FilterResonance, 0.3),
        (P::FilterEnvAmount, 0.3),
        (P::AmpAttack, 0.02),
        (P::AmpDecay, 0.3),
        (P::AmpSustain, 0.8),
        (P::AmpRelease, 0.3),
        (P::LfoRate, 5.5),
        (P::LfoDepth, 0.15),
        (P::LfoTarget, 0.0),
        (P::Glide, 0.08),
        (P::DriveAmount, 0.3),
        (P::DriveMix, 0.4),
        (P::DelayTime, 1.0),
        (P::DelayFeedback, 0.35),
        (P::DelayMix, 0.3),
        (P::ReverbSize, 0.6),
        (P::ReverbMix, 0.25),
    ],
};

/// Short percussive pluck: instant attack, no sustain, filter snaps shut.
pub const PLUCK: Preset = Preset {
    name: "Pluck",
    category: Category::EdmPlucks,
    tags: &["edm", "pluck", "classic", "short"],
    values: &[
        (P::Osc1Wave, 1.0),
        (P::Osc1Mix, 0.6),
        (P::Osc2Wave, 2.0),
        (P::Osc2Detune, 3.0),
        (P::Osc2Mix, 0.4),
        (P::FilterCutoff, 600.0),
        (P::FilterResonance, 0.4),
        (P::FilterEnvAmount, 1.0),
        (P::AmpAttack, 0.001),
        (P::AmpDecay, 0.18),
        (P::AmpSustain, 0.0),
        (P::AmpRelease, 0.18),
        (P::LfoDepth, 0.0),
        (P::DriveAmount, 0.2),
        (P::DriveMix, 0.3),
        (P::DelayTime, 0.0),
        (P::DelayFeedback, 0.4),
        (P::DelayMix, 0.3),
        (P::ReverbSize, 0.7),
        (P::ReverbMix, 0.3),
    ],
};

/// Slow, wide detuned pad with a gentle cutoff LFO and a big reverb.
pub const PAD: Preset = Preset {
    name: "Pad",
    category: Category::EdmPads,
    tags: &["edm", "pad", "classic", "wide", "slow"],
    values: &[
        (P::Osc1Wave, 1.0),
        (P::Osc1Detune, -12.0),
        (P::Osc1Mix, 0.6),
        (P::Osc2Wave, 1.0),
        (P::Osc2Detune, 12.0),
        (P::Osc2Mix, 0.6),
        (P::FilterCutoff, 1200.0),
        (P::FilterResonance, 0.15),
        (P::FilterEnvAmount, 0.25),
        (P::AmpAttack, 0.8),
        (P::AmpDecay, 1.0),
        (P::AmpSustain, 0.8),
        (P::AmpRelease, 1.5),
        (P::LfoRate, 0.4),
        (P::LfoDepth, 0.35),
        (P::LfoTarget, 1.0),
        (P::DriveAmount, 0.0),
        (P::DriveMix, 0.0),
        (P::DelayTime, 2.0),
        (P::DelayFeedback, 0.3),
        (P::DelayMix, 0.2),
        (P::ReverbSize, 0.9),
        (P::ReverbDamping, 0.3),
        (P::ReverbMix, 0.5),
    ],
};

/// Electric-piano-like keys: triangle + sine, quick attack, medium decay, light vibrato.
pub const KEYS: Preset = Preset {
    name: "Keys",
    category: Category::LofiKeys,
    tags: &["keys", "ep", "classic", "clean"],
    values: &[
        (P::Osc1Wave, 3.0),
        (P::Osc1Mix, 0.8),
        (P::Osc2Wave, 0.0),
        (P::Osc2Detune, 4.0),
        (P::Osc2Mix, 0.5),
        (P::FilterCutoff, 4000.0),
        (P::FilterResonance, 0.1),
        (P::FilterEnvAmount, 0.4),
        (P::AmpAttack, 0.003),
        (P::AmpDecay, 0.6),
        (P::AmpSustain, 0.35),
        (P::AmpRelease, 0.4),
        (P::LfoRate, 5.0),
        (P::LfoDepth, 0.08),
        (P::LfoTarget, 0.0),
        (P::DriveAmount, 0.15),
        (P::DriveMix, 0.3),
        (P::DelayTime, 1.0),
        (P::DelayMix, 0.15),
        (P::ReverbSize, 0.5),
        (P::ReverbMix, 0.3),
    ],
};

/// 303-style acid line: single saw, screaming resonance, deep filter envelope, glide, drive.
pub const ACID: Preset = Preset {
    name: "Acid",
    category: Category::EdmBass,
    tags: &["edm", "bass", "acid", "techno", "classic", "303"],
    values: &[
        (P::Osc1Wave, 1.0),
        (P::Osc1Mix, 1.0),
        (P::Osc2Mix, 0.0),
        (P::FilterCutoff, 350.0),
        (P::FilterResonance, 0.85),
        (P::FilterEnvAmount, 0.9),
        (P::AmpAttack, 0.002),
        (P::AmpDecay, 0.22),
        (P::AmpSustain, 0.1),
        (P::AmpRelease, 0.1),
        (P::LfoDepth, 0.0),
        (P::Glide, 0.12),
        (P::DriveAmount, 0.7),
        (P::DriveMix, 0.8),
        (P::DelayTime, 0.0),
        (P::DelayFeedback, 0.45),
        (P::DelayMix, 0.25),
        (P::ReverbMix, 0.1),
    ],
};

/// Bell: triangle plus a strongly detuned sine for shimmer, no sustain, long decay/release.
pub const BELL: Preset = Preset {
    name: "Bell",
    category: Category::EdmPlucks,
    tags: &["pluck", "bell", "classic", "bright", "mallet"],
    values: &[
        (P::Osc1Wave, 3.0),
        (P::Osc1Detune, 0.0),
        (P::Osc1Mix, 0.6),
        (P::Osc2Wave, 0.0),
        (P::Osc2Detune, -45.0),
        (P::Osc2Mix, 0.6),
        (P::FilterCutoff, 8000.0),
        (P::FilterResonance, 0.5),
        (P::FilterEnvAmount, 0.6),
        (P::AmpAttack, 0.001),
        (P::AmpDecay, 1.2),
        (P::AmpSustain, 0.0),
        (P::AmpRelease, 1.5),
        (P::LfoRate, 6.0),
        (P::LfoDepth, 0.05),
        (P::LfoTarget, 0.0),
        (P::DriveAmount, 0.0),
        (P::DriveMix, 0.0),
        (P::DelayTime, 1.0),
        (P::DelayFeedback, 0.5),
        (P::DelayMix, 0.35),
        (P::ReverbSize, 0.85),
        (P::ReverbDamping, 0.5),
        (P::ReverbMix, 0.5),
    ],
};

/// The original eight presets, in menu order (the synth panel's preset row).
pub const ALL: &[Preset] = &[BASS, SUB, LEAD, PLUCK, PAD, KEYS, ACID, BELL];

// --- the library (M9) -------------------------------------------------------------------
//
// Reading guide for the comments: every preset names its sound-design idea; the numbers are
// chosen against the DSP (voice.rs, fx/lofi.rs, fx/chorus.rs, fx/drive.rs, fx/delay.rs,
// fx/reverb.rs). Decay budget: the amp release ends the voice (-80 dB) after `AmpRelease`
// seconds; a synced delay at 120 BPM (1/16 = 0.125 s .. 1/2 = 1 s) must repeat about 64,
// 32, 16 or 8 times within 8 s, so the feedback stays <= 0.7 / 0.6 / 0.5 / 0.3 for delay
// times 0 / 1 / 2 / 3; the reverb's comb feedback is 0.7 + 0.28 x size, so sizes <= 0.9
// die within 8 s and 1.0 does not (drone). Wave indices: 0 sine, 1 saw, 2 square, 3
// triangle. Filter types: 0 LP, 1 BP, 2 HP. LFO target: 0 pitch, 1 cutoff.

/// The preset library, grouped by [`Category`] in [`Category::ALL`] order.
pub static LIBRARY: &[Preset] = &[
    // ----- Lofi Keys -----------------------------------------------------------------------
    // Dusty Rhodes: FM electric piano (sine carrier, 1:1 sine modulator at index ~1.8) with a
    // hammer click from the pitch envelope, a mellowing filter envelope, 11-bit / half-rate
    // lo-fi, tape wobble, vinyl hiss and a soft chorus.
    Preset {
        name: "Dusty Rhodes",
        category: Category::LofiKeys,
        tags: &["lofi", "ep", "keys", "rhodes", "warm", "vinyl", "joji"],
        values: values![
            Osc1Wave: 0.0, Osc1Detune: 0.0, Osc1Mix: 0.9, Osc1Unison: 1.0,
            Osc2Wave: 3.0, Osc2Detune: 3.0, Osc2Mix: 0.2,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.22, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 3500.0, FilterResonance: 0.1, FilterEnvAmount: 0.35,
            FilterAttack: 0.002, FilterDecay: 0.5, FilterSustain: 0.2, FilterRelease: 0.4, FilterKeyTrack: 0.4,
            AmpAttack: 0.003, AmpDecay: 0.9, AmpSustain: 0.3, AmpRelease: 0.5,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 2.0, PitchEnvDecay: 0.015, VelToCutoff: 0.6, VelToAmp: 1.0,
            DriveAmount: 0.1, DriveMix: 0.2,
            LofiBits: 11.0, LofiRate: 2.0, LofiWobble: 0.3, LofiNoise: 0.3, LofiTone: 6000.0, LofiMix: 0.8,
            ChorusRate: 0.5, ChorusDepth: 0.25, ChorusMix: 0.3, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.1, ReverbSize: 0.6, ReverbDamping: 0.5, ReverbMix: 0.3,
        ],
    },
    // Bedroom Keys: softer sine + triangle keys with less FM, a darker tone, more wobble and
    // 10-bit / third-rate grit; the chorus is wider and slower.
    Preset {
        name: "Bedroom Keys",
        category: Category::LofiKeys,
        tags: &["lofi", "keys", "ep", "soft", "warm", "vinyl", "bedroom"],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 0.8, Osc2Wave: 3.0, Osc2Detune: -5.0, Osc2Mix: 0.4,
            SubLevel: 0.1, NoiseLevel: 0.0, FmAmount: 0.12, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 2600.0, FilterResonance: 0.08, FilterEnvAmount: 0.3,
            FilterAttack: 0.003, FilterDecay: 0.7, FilterSustain: 0.25, FilterRelease: 0.5, FilterKeyTrack: 0.4,
            AmpAttack: 0.005, AmpDecay: 1.1, AmpSustain: 0.35, AmpRelease: 0.6,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 1.0, PitchEnvDecay: 0.012, VelToCutoff: 0.5, VelToAmp: 1.0,
            DriveAmount: 0.05, DriveMix: 0.15,
            LofiBits: 10.0, LofiRate: 3.0, LofiWobble: 0.45, LofiNoise: 0.4, LofiTone: 4500.0, LofiMix: 0.85,
            ChorusRate: 0.35, ChorusDepth: 0.35, ChorusMix: 0.4, SidechainAmount: 0.0,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.12, ReverbSize: 0.65, ReverbDamping: 0.6, ReverbMix: 0.35,
        ],
    },
    // Tape Wurli: reedier Wurlitzer bark from a 2:1 FM modulator and some drive, a slight
    // cutoff tremolo, heavier wobble and a gentle tape high cut at 5 kHz.
    Preset {
        name: "Tape Wurli",
        category: Category::LofiKeys,
        tags: &["lofi", "ep", "keys", "wurlitzer", "tape", "warm", "bark"],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 0.85, Osc2Wave: 0.0, Osc2Detune: -4.0, Osc2Mix: 0.3,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.3, FmRatio: 2.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 2800.0, FilterResonance: 0.12, FilterEnvAmount: 0.4,
            FilterAttack: 0.002, FilterDecay: 0.35, FilterSustain: 0.2, FilterRelease: 0.4, FilterKeyTrack: 0.4,
            AmpAttack: 0.003, AmpDecay: 1.2, AmpSustain: 0.25, AmpRelease: 0.45,
            LfoTarget: 1.0, LfoRate: 5.5, LfoDepth: 0.1, Glide: 0.0,
            PitchEnvAmount: 1.5, PitchEnvDecay: 0.01, VelToCutoff: 0.7, VelToAmp: 1.0,
            DriveAmount: 0.35, DriveMix: 0.4,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.5, LofiNoise: 0.15, LofiTone: 5000.0, LofiMix: 0.7,
            ChorusRate: 0.8, ChorusDepth: 0.2, ChorusMix: 0.2, SidechainAmount: 0.0,
            DelayTime: 1.0, DelayFeedback: 0.25, DelayMix: 0.1, ReverbSize: 0.55, ReverbDamping: 0.5, ReverbMix: 0.25,
        ],
    },
    // Lofi Piano: triangle body plus a touch of saw for hammer brightness, a 2:1 FM edge,
    // a 3-semitone hammer click, a long decay with little sustain, key-tracked lowpass,
    // 11-bit / half-rate lo-fi with wobble 0.3 and vinyl noise 0.3, a room reverb.
    Preset {
        name: "Lofi Piano",
        category: Category::LofiKeys,
        tags: &["lofi", "piano", "keys", "warm", "vinyl", "joji", "hip hop"],
        values: values![
            Osc1Wave: 3.0, Osc1Mix: 0.8, Osc2Wave: 1.0, Osc2Detune: 2.0, Osc2Mix: 0.15,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.15, FmRatio: 2.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 4200.0, FilterResonance: 0.05, FilterEnvAmount: 0.5,
            FilterAttack: 0.001, FilterDecay: 0.6, FilterSustain: 0.15, FilterRelease: 0.4, FilterKeyTrack: 0.5,
            AmpAttack: 0.002, AmpDecay: 1.5, AmpSustain: 0.2, AmpRelease: 0.4,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 3.0, PitchEnvDecay: 0.01, VelToCutoff: 0.7, VelToAmp: 1.0,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 11.0, LofiRate: 2.0, LofiWobble: 0.3, LofiNoise: 0.3, LofiTone: 6500.0, LofiMix: 0.75,
            ChorusRate: 0.6, ChorusDepth: 0.15, ChorusMix: 0.15, SidechainAmount: 0.0,
            DelayTime: 1.0, DelayFeedback: 0.25, DelayMix: 0.08, ReverbSize: 0.65, ReverbDamping: 0.5, ReverbMix: 0.3,
        ],
    },
    // Cassette Keys: the dullest keys here: 9 bits, quarter rate, strong wobble, 3.5 kHz
    // high cut, a sine/triangle pair with no FM; played soft the filter closes further.
    Preset {
        name: "Cassette Keys",
        category: Category::LofiKeys,
        tags: &["lofi", "keys", "tape", "cassette", "dark", "warm", "wobble"],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 0.9, Osc2Wave: 3.0, Osc2Detune: 6.0, Osc2Mix: 0.45,
            SubLevel: 0.15, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 2000.0, FilterResonance: 0.1, FilterEnvAmount: 0.3,
            FilterAttack: 0.003, FilterDecay: 0.5, FilterSustain: 0.3, FilterRelease: 0.4, FilterKeyTrack: 0.3,
            AmpAttack: 0.004, AmpDecay: 1.0, AmpSustain: 0.4, AmpRelease: 0.5,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 1.0, PitchEnvDecay: 0.01, VelToCutoff: 0.6, VelToAmp: 1.0,
            DriveAmount: 0.15, DriveMix: 0.25,
            LofiBits: 9.0, LofiRate: 4.0, LofiWobble: 0.6, LofiNoise: 0.35, LofiTone: 3500.0, LofiMix: 0.9,
            ChorusRate: 0.4, ChorusDepth: 0.3, ChorusMix: 0.2, SidechainAmount: 0.0,
            DelayTime: 1.0, DelayFeedback: 0.2, DelayMix: 0.05, ReverbSize: 0.5, ReverbDamping: 0.7, ReverbMix: 0.25,
        ],
    },
    // Rainy Day EP: 1:1 FM at a higher index for a glassier bark, a slow 0.3 Hz cutoff
    // sway, heavy vinyl rain (noise 0.5), a long reverb and a quarter-note echo.
    Preset {
        name: "Rainy Day EP",
        category: Category::LofiKeys,
        tags: &[
            "lofi", "ep", "keys", "rain", "vinyl", "sad", "joji", "mellow",
        ],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 0.9, Osc2Wave: 0.0, Osc2Detune: 7.0, Osc2Mix: 0.25,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.28, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 2500.0, FilterResonance: 0.15, FilterEnvAmount: 0.3,
            FilterAttack: 0.002, FilterDecay: 0.6, FilterSustain: 0.2, FilterRelease: 0.5, FilterKeyTrack: 0.4,
            AmpAttack: 0.02, AmpDecay: 1.3, AmpSustain: 0.3, AmpRelease: 0.8,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.15, Glide: 0.0,
            PitchEnvAmount: 1.5, PitchEnvDecay: 0.012, VelToCutoff: 0.6, VelToAmp: 1.0,
            DriveAmount: 0.1, DriveMix: 0.2,
            LofiBits: 12.0, LofiRate: 2.0, LofiWobble: 0.25, LofiNoise: 0.5, LofiTone: 7000.0, LofiMix: 0.7,
            ChorusRate: 0.45, ChorusDepth: 0.3, ChorusMix: 0.35, SidechainAmount: 0.0,
            DelayTime: 2.0, DelayFeedback: 0.4, DelayMix: 0.2, ReverbSize: 0.8, ReverbDamping: 0.4, ReverbMix: 0.45,
        ],
    },
    // Glass EP: DX-style 4:1 FM at a modest index gives glassy upper partials; only mild
    // lo-fi (12 bits, full rate), a wider chorus and a longer release.
    Preset {
        name: "Glass EP",
        category: Category::LofiKeys,
        tags: &["lofi", "ep", "keys", "fm", "glass", "bright", "dx"],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 0.9, Osc2Wave: 0.0, Osc2Detune: 5.0, Osc2Mix: 0.3,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.2, FmRatio: 4.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 6000.0, FilterResonance: 0.1, FilterEnvAmount: 0.4,
            FilterAttack: 0.001, FilterDecay: 0.4, FilterSustain: 0.2, FilterRelease: 0.6, FilterKeyTrack: 0.5,
            AmpAttack: 0.002, AmpDecay: 1.4, AmpSustain: 0.25, AmpRelease: 1.0,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.6, VelToAmp: 1.0,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.15, LofiNoise: 0.15, LofiTone: 9000.0, LofiMix: 0.5,
            ChorusRate: 0.7, ChorusDepth: 0.4, ChorusMix: 0.4, SidechainAmount: 0.0,
            DelayTime: 1.0, DelayFeedback: 0.35, DelayMix: 0.2, ReverbSize: 0.7, ReverbDamping: 0.4, ReverbMix: 0.35,
        ],
    },
    // Toy Piano: a narrow pulse with a detuned sine partner for the tinny beating, no
    // sustain, a 5-semitone click, 10-bit / third-rate lo-fi.
    Preset {
        name: "Toy Piano",
        category: Category::LofiKeys,
        tags: &[
            "lofi",
            "piano",
            "keys",
            "toy",
            "bright",
            "cute",
            "music box",
        ],
        values: values![
            Osc1Wave: 2.0, Osc1Pulse: 0.3, Osc1Mix: 0.6, Osc2Wave: 0.0, Osc2Detune: 12.0, Osc2Mix: 0.4,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 6000.0, FilterResonance: 0.15, FilterEnvAmount: 0.4,
            FilterAttack: 0.001, FilterDecay: 0.3, FilterSustain: 0.1, FilterRelease: 0.3, FilterKeyTrack: 0.5,
            AmpAttack: 0.001, AmpDecay: 0.8, AmpSustain: 0.0, AmpRelease: 0.3,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 5.0, PitchEnvDecay: 0.008, VelToCutoff: 0.4, VelToAmp: 1.0,
            DriveAmount: 0.1, DriveMix: 0.2,
            LofiBits: 10.0, LofiRate: 3.0, LofiWobble: 0.2, LofiNoise: 0.2, LofiTone: 8000.0, LofiMix: 0.7,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.15, SidechainAmount: 0.0,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.15, ReverbSize: 0.6, ReverbDamping: 0.5, ReverbMix: 0.3,
        ],
    },
    // Organ Dust: square + sine drawbars, full sustain, no filter envelope, a 6 Hz pitch
    // vibrato standing in for a Leslie, a fast chorus, light drive and vinyl dust.
    Preset {
        name: "Organ Dust",
        category: Category::LofiKeys,
        tags: &["lofi", "keys", "organ", "vinyl", "warm", "gospel", "soul"],
        values: values![
            Osc1Wave: 2.0, Osc1Pulse: 0.5, Osc1Mix: 0.6, Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.5,
            SubLevel: 0.3, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 3000.0, FilterResonance: 0.05, FilterEnvAmount: 0.05,
            FilterAttack: 0.005, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.3,
            AmpAttack: 0.01, AmpDecay: 0.2, AmpSustain: 1.0, AmpRelease: 0.1,
            LfoTarget: 0.0, LfoRate: 6.0, LfoDepth: 0.06, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.5,
            DriveAmount: 0.3, DriveMix: 0.4,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.2, LofiNoise: 0.25, LofiTone: 5000.0, LofiMix: 0.6,
            ChorusRate: 1.2, ChorusDepth: 0.4, ChorusMix: 0.5, SidechainAmount: 0.0,
            DelayTime: 1.0, DelayFeedback: 0.2, DelayMix: 0.05, ReverbSize: 0.6, ReverbDamping: 0.5, ReverbMix: 0.25,
        ],
    },
    // Music Box: sine with a 3.5:1 FM modulator for inharmonic bell partials, one-second
    // decay with no sustain, high key tracking, an eighth-note echo and a long room.
    Preset {
        name: "Music Box",
        category: Category::LofiKeys,
        tags: &[
            "lofi",
            "keys",
            "music box",
            "bell",
            "bright",
            "cute",
            "mallet",
        ],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 0.9, Osc2Wave: 3.0, Osc2Detune: 0.0, Osc2Mix: 0.2,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.25, FmRatio: 3.5, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 8000.0, FilterResonance: 0.1, FilterEnvAmount: 0.3,
            FilterAttack: 0.001, FilterDecay: 0.3, FilterSustain: 0.2, FilterRelease: 0.5, FilterKeyTrack: 0.8,
            AmpAttack: 0.001, AmpDecay: 1.0, AmpSustain: 0.0, AmpRelease: 0.8,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 1.0,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.15, LofiNoise: 0.3, LofiTone: 10000.0, LofiMix: 0.5,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.2, SidechainAmount: 0.0,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.2, ReverbSize: 0.7, ReverbDamping: 0.4, ReverbMix: 0.4,
        ],
    },
    // Vapor Keys: triangle + a 7-cent-flat sine with a slow attack and long release, a slow
    // deep chorus, half-rate 10-bit lo-fi and a very slow cutoff sway: vaporwave keys.
    Preset {
        name: "Vapor Keys",
        category: Category::LofiKeys,
        tags: &[
            "lofi",
            "keys",
            "vaporwave",
            "chorus",
            "dreamy",
            "warm",
            "wobble",
        ],
        values: values![
            Osc1Wave: 3.0, Osc1Mix: 0.8, Osc2Wave: 0.0, Osc2Detune: -7.0, Osc2Mix: 0.5,
            SubLevel: 0.1, NoiseLevel: 0.0, FmAmount: 0.05, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 3200.0, FilterResonance: 0.1, FilterEnvAmount: 0.2,
            FilterAttack: 0.05, FilterDecay: 0.8, FilterSustain: 0.4, FilterRelease: 0.8, FilterKeyTrack: 0.3,
            AmpAttack: 0.05, AmpDecay: 1.5, AmpSustain: 0.4, AmpRelease: 1.2,
            LfoTarget: 1.0, LfoRate: 0.25, LfoDepth: 0.2, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.4, VelToAmp: 0.8,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 10.0, LofiRate: 2.0, LofiWobble: 0.4, LofiNoise: 0.2, LofiTone: 7000.0, LofiMix: 0.7,
            ChorusRate: 0.4, ChorusDepth: 0.5, ChorusMix: 0.6, SidechainAmount: 0.0,
            DelayTime: 2.0, DelayFeedback: 0.4, DelayMix: 0.2, ReverbSize: 0.8, ReverbDamping: 0.4, ReverbMix: 0.4,
        ],
    },
    // ----- Lofi Guitars --------------------------------------------------------------------
    // Nylon Guitar: Karplus-Strong string, bright excitation (0.8), medium-short damping so
    // the body rings about a second, a key-tracked lowpass with no envelope, a light slow
    // chorus, a little vinyl, and a short release so the note stops when the key lifts.
    Preset {
        name: "Nylon Guitar",
        category: Category::LofiGuitars,
        tags: &[
            "lofi", "guitar", "nylon", "pluck", "acoustic", "warm", "joji",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 1.0, Osc2Mix: 0.0, SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0,
            OscMode: 1.0, PluckDamping: 0.55, PluckBright: 0.8,
            FilterType: 0.0, FilterCutoff: 5000.0, FilterResonance: 0.05, FilterEnvAmount: 0.0,
            FilterAttack: 0.001, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.5,
            AmpAttack: 0.001, AmpDecay: 0.3, AmpSustain: 1.0, AmpRelease: 0.35,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.4, VelToAmp: 1.0,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.1, LofiNoise: 0.15, LofiTone: 8000.0, LofiMix: 0.4,
            ChorusRate: 0.6, ChorusDepth: 0.2, ChorusMix: 0.2, SidechainAmount: 0.0,
            DelayTime: 1.0, DelayFeedback: 0.25, DelayMix: 0.08, ReverbSize: 0.55, ReverbDamping: 0.5, ReverbMix: 0.25,
        ],
    },
    // Bedroom Strat: clean electric: a less bright, longer-ringing string through a light
    // amp drive, a wider chorus, eighth-note slapback and tape wobble.
    Preset {
        name: "Bedroom Strat",
        category: Category::LofiGuitars,
        tags: &[
            "lofi", "guitar", "electric", "clean", "pluck", "indie", "bedroom",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 1.0, Osc2Mix: 0.0, SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0,
            OscMode: 1.0, PluckDamping: 0.4, PluckBright: 0.6,
            FilterType: 0.0, FilterCutoff: 3500.0, FilterResonance: 0.1, FilterEnvAmount: 0.0,
            FilterAttack: 0.001, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.4,
            AmpAttack: 0.001, AmpDecay: 0.3, AmpSustain: 1.0, AmpRelease: 0.5,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 1.0,
            DriveAmount: 0.25, DriveMix: 0.35,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.2, LofiNoise: 0.2, LofiTone: 7000.0, LofiMix: 0.5,
            ChorusRate: 0.9, ChorusDepth: 0.35, ChorusMix: 0.35, SidechainAmount: 0.0,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.2, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.3,
        ],
    },
    // Muted Pluck Gtr: palm-muted string: damping 0.9 (quarter-second ring), darker
    // excitation, a 2.5 kHz lowpass and a very short release; dry-ish with vinyl dust.
    Preset {
        name: "Muted Pluck Gtr",
        category: Category::LofiGuitars,
        tags: &[
            "lofi", "guitar", "muted", "pluck", "short", "dark", "hip hop",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 1.0, Osc2Mix: 0.0, SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0,
            OscMode: 1.0, PluckDamping: 0.9, PluckBright: 0.5,
            FilterType: 0.0, FilterCutoff: 2500.0, FilterResonance: 0.1, FilterEnvAmount: 0.2,
            FilterAttack: 0.001, FilterDecay: 0.1, FilterSustain: 0.0, FilterRelease: 0.2, FilterKeyTrack: 0.4,
            AmpAttack: 0.001, AmpDecay: 0.2, AmpSustain: 0.8, AmpRelease: 0.2,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.5, VelToAmp: 1.0,
            DriveAmount: 0.15, DriveMix: 0.3,
            LofiBits: 11.0, LofiRate: 2.0, LofiWobble: 0.15, LofiNoise: 0.25, LofiTone: 6000.0, LofiMix: 0.6,
            ChorusRate: 0.5, ChorusDepth: 0.15, ChorusMix: 0.1, SidechainAmount: 0.0,
            DelayTime: 0.0, DelayFeedback: 0.3, DelayMix: 0.1, ReverbSize: 0.4, ReverbDamping: 0.6, ReverbMix: 0.15,
        ],
    },
    // Jazz Hollowbody: dark excitation (0.3) into a 1.8 kHz lowpass, a little warmth from
    // the drive, no chorus, vinyl dust and a 4 kHz tape cut; a small room.
    Preset {
        name: "Jazz Hollowbody",
        category: Category::LofiGuitars,
        tags: &[
            "lofi",
            "guitar",
            "jazz",
            "dark",
            "warm",
            "pluck",
            "hollowbody",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 1.0, Osc2Mix: 0.0, SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0,
            OscMode: 1.0, PluckDamping: 0.6, PluckBright: 0.3,
            FilterType: 0.0, FilterCutoff: 1800.0, FilterResonance: 0.15, FilterEnvAmount: 0.1,
            FilterAttack: 0.001, FilterDecay: 0.2, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.4,
            AmpAttack: 0.001, AmpDecay: 0.3, AmpSustain: 1.0, AmpRelease: 0.4,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.5, VelToAmp: 1.0,
            DriveAmount: 0.15, DriveMix: 0.3,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.15, LofiNoise: 0.25, LofiTone: 4000.0, LofiMix: 0.6,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0,
            DelayTime: 1.0, DelayFeedback: 0.2, DelayMix: 0.05, ReverbSize: 0.5, ReverbDamping: 0.6, ReverbMix: 0.2,
        ],
    },
    // Tape Telecaster: the brightest string (0.9) with a touch of resonance, more drive,
    // strong tape wobble, half-rate 11-bit lo-fi and a quarter-note echo.
    Preset {
        name: "Tape Telecaster",
        category: Category::LofiGuitars,
        tags: &[
            "lofi", "guitar", "electric", "bright", "tape", "twang", "pluck",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 1.0, Osc2Mix: 0.0, SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0,
            OscMode: 1.0, PluckDamping: 0.45, PluckBright: 0.9,
            FilterType: 0.0, FilterCutoff: 4500.0, FilterResonance: 0.15, FilterEnvAmount: 0.0,
            FilterAttack: 0.001, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.5,
            AmpAttack: 0.001, AmpDecay: 0.3, AmpSustain: 1.0, AmpRelease: 0.45,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 1.0,
            DriveAmount: 0.4, DriveMix: 0.5,
            LofiBits: 11.0, LofiRate: 2.0, LofiWobble: 0.5, LofiNoise: 0.2, LofiTone: 6000.0, LofiMix: 0.7,
            ChorusRate: 0.7, ChorusDepth: 0.25, ChorusMix: 0.25, SidechainAmount: 0.0,
            DelayTime: 2.0, DelayFeedback: 0.35, DelayMix: 0.25, ReverbSize: 0.55, ReverbDamping: 0.5, ReverbMix: 0.25,
        ],
    },
    // Ukulele Lofi: short bright string through a 350 Hz highpass (thin little body) and
    // 10-bit / half-rate lo-fi with crackle; a quick release.
    Preset {
        name: "Ukulele Lofi",
        category: Category::LofiGuitars,
        tags: &[
            "lofi", "guitar", "ukulele", "bright", "pluck", "cute", "short",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 1.0, Osc2Mix: 0.0, SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0,
            OscMode: 1.0, PluckDamping: 0.7, PluckBright: 0.85,
            FilterType: 2.0, FilterCutoff: 350.0, FilterResonance: 0.2, FilterEnvAmount: 0.0,
            FilterAttack: 0.001, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.5,
            AmpAttack: 0.001, AmpDecay: 0.3, AmpSustain: 1.0, AmpRelease: 0.3,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 1.0,
            DriveAmount: 0.05, DriveMix: 0.15,
            LofiBits: 10.0, LofiRate: 2.0, LofiWobble: 0.2, LofiNoise: 0.3, LofiTone: 9000.0, LofiMix: 0.6,
            ChorusRate: 0.6, ChorusDepth: 0.15, ChorusMix: 0.15, SidechainAmount: 0.0,
            DelayTime: 0.0, DelayFeedback: 0.3, DelayMix: 0.1, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.25,
        ],
    },
    // Harp Dust: a long-ringing string (damping 0.15, ~6 s) with a 2.5 s amp release so it
    // sings after the key lifts, a big reverb and a quarter-note echo under vinyl dust.
    Preset {
        name: "Harp Dust",
        category: Category::LofiGuitars,
        tags: &["lofi", "harp", "pluck", "dreamy", "vinyl", "long", "guitar"],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 1.0, Osc2Mix: 0.0, SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0,
            OscMode: 1.0, PluckDamping: 0.15, PluckBright: 0.7,
            FilterType: 0.0, FilterCutoff: 6000.0, FilterResonance: 0.05, FilterEnvAmount: 0.0,
            FilterAttack: 0.001, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.5,
            AmpAttack: 0.001, AmpDecay: 0.3, AmpSustain: 1.0, AmpRelease: 2.5,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 1.0,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.2, LofiNoise: 0.3, LofiTone: 9000.0, LofiMix: 0.5,
            ChorusRate: 0.4, ChorusDepth: 0.25, ChorusMix: 0.25, SidechainAmount: 0.0,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.2, ReverbSize: 0.8, ReverbDamping: 0.4, ReverbMix: 0.45,
        ],
    },
    // Koto Lofi: raw-noise excitation (bright 1.0) for a metallic attack, a 1-semitone
    // bend from the pitch envelope, 10-bit / third-rate grit and a feeding-back eighth echo.
    Preset {
        name: "Koto Lofi",
        category: Category::LofiGuitars,
        tags: &["lofi", "koto", "guitar", "pluck", "bright", "asian", "trap"],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 1.0, Osc2Mix: 0.0, SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0,
            OscMode: 1.0, PluckDamping: 0.5, PluckBright: 1.0,
            FilterType: 0.0, FilterCutoff: 8000.0, FilterResonance: 0.1, FilterEnvAmount: 0.0,
            FilterAttack: 0.001, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.5,
            AmpAttack: 0.001, AmpDecay: 0.3, AmpSustain: 1.0, AmpRelease: 0.4,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 1.0, PitchEnvDecay: 0.03, VelToCutoff: 0.3, VelToAmp: 1.0,
            DriveAmount: 0.1, DriveMix: 0.2,
            LofiBits: 10.0, LofiRate: 3.0, LofiWobble: 0.3, LofiNoise: 0.25, LofiTone: 8000.0, LofiMix: 0.6,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.1, SidechainAmount: 0.0,
            DelayTime: 1.0, DelayFeedback: 0.5, DelayMix: 0.3, ReverbSize: 0.6, ReverbDamping: 0.5, ReverbMix: 0.3,
        ],
    },
    // Slow Swell Gtr: volume-pedal swell: a long-ringing string under a 0.6 s amp attack
    // and 1.5 s release, a slow deep chorus, a long reverb and a quarter-note echo.
    Preset {
        name: "Slow Swell Gtr",
        category: Category::LofiGuitars,
        tags: &[
            "lofi", "guitar", "swell", "ambient", "dreamy", "slow", "pad",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 1.0, Osc2Mix: 0.0, SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0,
            OscMode: 1.0, PluckDamping: 0.2, PluckBright: 0.6,
            FilterType: 0.0, FilterCutoff: 3000.0, FilterResonance: 0.1, FilterEnvAmount: 0.0,
            FilterAttack: 0.001, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.4,
            AmpAttack: 0.6, AmpDecay: 0.3, AmpSustain: 1.0, AmpRelease: 1.5,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.7,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.3, LofiNoise: 0.2, LofiTone: 7000.0, LofiMix: 0.5,
            ChorusRate: 0.3, ChorusDepth: 0.4, ChorusMix: 0.5, SidechainAmount: 0.0,
            DelayTime: 2.0, DelayFeedback: 0.45, DelayMix: 0.3, ReverbSize: 0.85, ReverbDamping: 0.4, ReverbMix: 0.5,
        ],
    },
    // Bass Guitar Lofi: a dark, medium-damped string through a 1.2 kHz lowpass and amp
    // drive, 3 kHz tape cut; meant an octave or two below middle C.
    Preset {
        name: "Bass Guitar Lofi",
        category: Category::LofiGuitars,
        tags: &[
            "lofi",
            "guitar",
            "bass",
            "dark",
            "warm",
            "pluck",
            "fingerstyle",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 1.0, Osc2Mix: 0.0, SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0,
            OscMode: 1.0, PluckDamping: 0.65, PluckBright: 0.4,
            FilterType: 0.0, FilterCutoff: 1200.0, FilterResonance: 0.1, FilterEnvAmount: 0.15,
            FilterAttack: 0.001, FilterDecay: 0.15, FilterSustain: 0.1, FilterRelease: 0.2, FilterKeyTrack: 0.3,
            AmpAttack: 0.001, AmpDecay: 0.3, AmpSustain: 1.0, AmpRelease: 0.3,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.5, VelToAmp: 1.0,
            DriveAmount: 0.3, DriveMix: 0.4,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.1, LofiNoise: 0.1, LofiTone: 3000.0, LofiMix: 0.5,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0,
            DelayTime: 1.0, DelayFeedback: 0.2, DelayMix: 0.0, ReverbSize: 0.4, ReverbDamping: 0.6, ReverbMix: 0.1,
        ],
    },
    // Dream Clean: shoegaze clean: long ring, a deep 0.9 Hz chorus at 70 %, only a hint of
    // lo-fi, a feeding-back eighth echo and a long reverb with a 0.8 s release.
    Preset {
        name: "Dream Clean",
        category: Category::LofiGuitars,
        tags: &[
            "lofi", "guitar", "clean", "chorus", "dreamy", "shoegaze", "pluck",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 1.0, Osc2Mix: 0.0, SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0,
            OscMode: 1.0, PluckDamping: 0.3, PluckBright: 0.7,
            FilterType: 0.0, FilterCutoff: 4500.0, FilterResonance: 0.1, FilterEnvAmount: 0.0,
            FilterAttack: 0.001, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.5,
            AmpAttack: 0.002, AmpDecay: 0.3, AmpSustain: 1.0, AmpRelease: 0.8,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 1.0,
            DriveAmount: 0.1, DriveMix: 0.2,
            LofiBits: 13.0, LofiRate: 1.0, LofiWobble: 0.15, LofiNoise: 0.15, LofiTone: 9000.0, LofiMix: 0.3,
            ChorusRate: 0.9, ChorusDepth: 0.6, ChorusMix: 0.7, SidechainAmount: 0.0,
            DelayTime: 1.0, DelayFeedback: 0.4, DelayMix: 0.3, ReverbSize: 0.75, ReverbDamping: 0.4, ReverbMix: 0.4,
        ],
    },
    // Tremolo Dirt: a fuzzed string (drive 0.6) under a 5 Hz cutoff LFO over a 1.5 kHz
    // lowpass, which chops the level like an amp tremolo; 10-bit lo-fi with crackle.
    Preset {
        name: "Tremolo Dirt",
        category: Category::LofiGuitars,
        tags: &[
            "lofi", "guitar", "tremolo", "dirty", "fuzz", "dark", "pluck",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 1.0, Osc2Mix: 0.0, SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0,
            OscMode: 1.0, PluckDamping: 0.4, PluckBright: 0.5,
            FilterType: 0.0, FilterCutoff: 1500.0, FilterResonance: 0.2, FilterEnvAmount: 0.0,
            FilterAttack: 0.001, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.4,
            AmpAttack: 0.001, AmpDecay: 0.3, AmpSustain: 1.0, AmpRelease: 0.5,
            LfoTarget: 1.0, LfoRate: 5.0, LfoDepth: 0.5, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 1.0,
            DriveAmount: 0.6, DriveMix: 0.7,
            LofiBits: 10.0, LofiRate: 1.0, LofiWobble: 0.2, LofiNoise: 0.3, LofiTone: 5000.0, LofiMix: 0.6,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.15, ReverbSize: 0.6, ReverbDamping: 0.5, ReverbMix: 0.3,
        ],
    },
    // ----- Lofi Pads -----------------------------------------------------------------------
    // Night Drive Pad: three detuned saws plus a narrow pulse and a sub, a 1.4 kHz lowpass
    // that opens over 1.5 s (filter attack) and sways at 0.2 Hz, a 1.2 s attack and 2.5 s
    // release, chorus, tape wobble and vinyl hiss, a long damped reverb.
    Preset {
        name: "Night Drive Pad",
        category: Category::LofiPads,
        tags: &["lofi", "pad", "warm", "dark", "night", "synthwave", "joji"],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.6, Osc1Unison: 3.0, Osc1Spread: 12.0,
            Osc2Wave: 2.0, Osc2Pulse: 0.3, Osc2Detune: -8.0, Osc2Mix: 0.35,
            SubLevel: 0.3, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 1400.0, FilterResonance: 0.15, FilterEnvAmount: 0.2,
            FilterAttack: 1.5, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.5, FilterKeyTrack: 0.3,
            AmpAttack: 1.2, AmpDecay: 1.0, AmpSustain: 0.85, AmpRelease: 2.5,
            LfoTarget: 1.0, LfoRate: 0.2, LfoDepth: 0.2, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 0.6,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.35, LofiNoise: 0.3, LofiTone: 5000.0, LofiMix: 0.7,
            ChorusRate: 0.4, ChorusDepth: 0.5, ChorusMix: 0.6, SidechainAmount: 0.0,
            DelayTime: 2.0, DelayFeedback: 0.4, DelayMix: 0.2, ReverbSize: 0.85, ReverbDamping: 0.5, ReverbMix: 0.5,
        ],
    },
    // Warm Tape Pad: triangles in unison with a sine partner, a 900 Hz lowpass, 1.5 s
    // attack and 3 s release, strong wobble, 11 bits, 3.5 kHz tape cut: all warmth.
    Preset {
        name: "Warm Tape Pad",
        category: Category::LofiPads,
        tags: &["lofi", "pad", "warm", "tape", "soft", "mellow", "wobble"],
        values: values![
            Osc1Wave: 3.0, Osc1Mix: 0.7, Osc1Unison: 3.0, Osc1Spread: 10.0,
            Osc2Wave: 0.0, Osc2Detune: 6.0, Osc2Mix: 0.5,
            SubLevel: 0.2, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 900.0, FilterResonance: 0.1, FilterEnvAmount: 0.15,
            FilterAttack: 1.0, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.5, FilterKeyTrack: 0.3,
            AmpAttack: 1.5, AmpDecay: 1.0, AmpSustain: 0.9, AmpRelease: 3.0,
            LfoTarget: 1.0, LfoRate: 0.15, LfoDepth: 0.15, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.6,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 11.0, LofiRate: 1.0, LofiWobble: 0.6, LofiNoise: 0.35, LofiTone: 3500.0, LofiMix: 0.85,
            ChorusRate: 0.3, ChorusDepth: 0.4, ChorusMix: 0.4, SidechainAmount: 0.0,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.15, ReverbSize: 0.85, ReverbDamping: 0.6, ReverbMix: 0.45,
        ],
    },
    // VHS Strings: five spread saws, a 2.2 kHz lowpass with a 0.8 s filter attack, a
    // 70 % chorus, half-rate 10-bit lo-fi with heavy wobble and hiss, 0.8 s / 2 s envelope.
    Preset {
        name: "VHS Strings",
        category: Category::LofiPads,
        tags: &[
            "lofi",
            "pad",
            "strings",
            "vhs",
            "retro",
            "wobble",
            "synthwave",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.6, Osc1Unison: 5.0, Osc1Spread: 18.0,
            Osc2Wave: 1.0, Osc2Detune: -9.0, Osc2Mix: 0.3,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 2200.0, FilterResonance: 0.1, FilterEnvAmount: 0.25,
            FilterAttack: 0.8, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.0, FilterKeyTrack: 0.3,
            AmpAttack: 0.8, AmpDecay: 1.0, AmpSustain: 0.9, AmpRelease: 2.0,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.1, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 0.7,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 10.0, LofiRate: 2.0, LofiWobble: 0.5, LofiNoise: 0.4, LofiTone: 6000.0, LofiMix: 0.8,
            ChorusRate: 0.5, ChorusDepth: 0.5, ChorusMix: 0.7, SidechainAmount: 0.0,
            DelayTime: 2.0, DelayFeedback: 0.35, DelayMix: 0.15, ReverbSize: 0.8, ReverbDamping: 0.5, ReverbMix: 0.45,
        ],
    },
    // Underwater Choir: a narrow pulse and triangle with a hint of 2:1 FM through a
    // resonant bandpass at 900 Hz that drifts at 0.15 Hz; near-full lo-fi with wobble
    // 0.7 and a 2.5 kHz cut, a deep 80 % chorus and a huge reverb.
    Preset {
        name: "Underwater Choir",
        category: Category::LofiPads,
        tags: &[
            "lofi",
            "pad",
            "choir",
            "dark",
            "underwater",
            "dreamy",
            "wobble",
        ],
        values: values![
            Osc1Wave: 2.0, Osc1Pulse: 0.2, Osc1Mix: 0.6, Osc1Unison: 2.0, Osc1Spread: 8.0,
            Osc2Wave: 3.0, Osc2Detune: 4.0, Osc2Mix: 0.5,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.1, FmRatio: 2.0, OscMode: 0.0,
            FilterType: 1.0, FilterCutoff: 900.0, FilterResonance: 0.4, FilterEnvAmount: 0.1,
            FilterAttack: 1.0, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.5, FilterKeyTrack: 0.3,
            AmpAttack: 1.0, AmpDecay: 1.0, AmpSustain: 0.9, AmpRelease: 2.5,
            LfoTarget: 1.0, LfoRate: 0.15, LfoDepth: 0.3, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.6,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.7, LofiNoise: 0.2, LofiTone: 2500.0, LofiMix: 0.9,
            ChorusRate: 0.25, ChorusDepth: 0.7, ChorusMix: 0.8, SidechainAmount: 0.0,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.15, ReverbSize: 0.9, ReverbDamping: 0.5, ReverbMix: 0.6,
        ],
    },
    // Dusty Organ Pad: square + sine with full sustain and no filter envelope, a slow
    // 0.3 s attack, 5.5 Hz vibrato, light drive, a 50 % chorus and vinyl dust.
    Preset {
        name: "Dusty Organ Pad",
        category: Category::LofiPads,
        tags: &["lofi", "pad", "organ", "vinyl", "warm", "church", "soul"],
        values: values![
            Osc1Wave: 2.0, Osc1Pulse: 0.5, Osc1Mix: 0.55, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.5,
            SubLevel: 0.4, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 2500.0, FilterResonance: 0.05, FilterEnvAmount: 0.0,
            FilterAttack: 0.005, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.3,
            AmpAttack: 0.3, AmpDecay: 0.5, AmpSustain: 1.0, AmpRelease: 1.0,
            LfoTarget: 0.0, LfoRate: 5.5, LfoDepth: 0.05, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.1, VelToAmp: 0.4,
            DriveAmount: 0.2, DriveMix: 0.3,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.3, LofiNoise: 0.4, LofiTone: 5000.0, LofiMix: 0.7,
            ChorusRate: 1.0, ChorusDepth: 0.4, ChorusMix: 0.5, SidechainAmount: 0.0,
            DelayTime: 1.0, DelayFeedback: 0.2, DelayMix: 0.05, ReverbSize: 0.75, ReverbDamping: 0.5, ReverbMix: 0.35,
        ],
    },
    // Bedroom Strings: a seven-voice supersaw kept quiet and dark (1.8 kHz) with a slow
    // filter attack, 1.5 s / 2 s envelope, moderate chorus and only mild lo-fi.
    Preset {
        name: "Bedroom Strings",
        category: Category::LofiPads,
        tags: &[
            "lofi",
            "pad",
            "strings",
            "warm",
            "cinematic",
            "bedroom",
            "joji",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.5, Osc1Unison: 7.0, Osc1Spread: 20.0,
            Osc2Wave: 1.0, Osc2Detune: -12.0, Osc2Mix: 0.3,
            SubLevel: 0.1, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 1800.0, FilterResonance: 0.1, FilterEnvAmount: 0.15,
            FilterAttack: 1.0, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.5, FilterKeyTrack: 0.3,
            AmpAttack: 1.5, AmpDecay: 1.0, AmpSustain: 0.8, AmpRelease: 2.0,
            LfoTarget: 1.0, LfoRate: 0.2, LfoDepth: 0.1, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 0.7,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.25, LofiNoise: 0.3, LofiTone: 7000.0, LofiMix: 0.6,
            ChorusRate: 0.5, ChorusDepth: 0.4, ChorusMix: 0.5, SidechainAmount: 0.0,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.15, ReverbSize: 0.85, ReverbDamping: 0.4, ReverbMix: 0.5,
        ],
    },
    // Hollow Moon: triangle + sine over a strong sub through a resonant 700 Hz lowpass,
    // 2 s attack and 3.5 s release, near-full lo-fi with a 3 kHz cut: a dark hollow bed.
    Preset {
        name: "Hollow Moon",
        category: Category::LofiPads,
        tags: &["lofi", "pad", "dark", "hollow", "ambient", "sub", "night"],
        values: values![
            Osc1Wave: 3.0, Osc1Mix: 0.7, Osc1Unison: 2.0, Osc1Spread: 6.0,
            Osc2Wave: 0.0, Osc2Detune: -3.0, Osc2Mix: 0.4,
            SubLevel: 0.5, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 700.0, FilterResonance: 0.3, FilterEnvAmount: 0.2,
            FilterAttack: 2.0, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 2.0, FilterKeyTrack: 0.3,
            AmpAttack: 2.0, AmpDecay: 1.0, AmpSustain: 0.9, AmpRelease: 3.5,
            LfoTarget: 1.0, LfoRate: 0.1, LfoDepth: 0.2, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.5,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.5, LofiNoise: 0.4, LofiTone: 3000.0, LofiMix: 0.9,
            ChorusRate: 0.3, ChorusDepth: 0.3, ChorusMix: 0.3, SidechainAmount: 0.0,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.1, ReverbSize: 0.9, ReverbDamping: 0.6, ReverbMix: 0.5,
        ],
    },
    // Lofi Flute Pad: a sine with a little 1:1 FM and pink breath noise, 5 Hz vibrato,
    // a 0.4 s attack, vinyl dust, a long reverb and a quarter-note echo.
    Preset {
        name: "Lofi Flute Pad",
        category: Category::LofiPads,
        tags: &[
            "lofi", "pad", "flute", "breathy", "soft", "warm", "woodwind",
        ],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 0.9, Osc1Unison: 1.0,
            Osc2Wave: 3.0, Osc2Detune: 2.0, Osc2Mix: 0.2,
            SubLevel: 0.0, NoiseLevel: 0.08, NoiseColor: 1.0, FmAmount: 0.08, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 2500.0, FilterResonance: 0.1, FilterEnvAmount: 0.2,
            FilterAttack: 0.3, FilterDecay: 0.5, FilterSustain: 0.8, FilterRelease: 0.8, FilterKeyTrack: 0.4,
            AmpAttack: 0.4, AmpDecay: 0.5, AmpSustain: 0.9, AmpRelease: 1.0,
            LfoTarget: 0.0, LfoRate: 5.0, LfoDepth: 0.08, Glide: 0.03,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 0.8,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.3, LofiNoise: 0.3, LofiTone: 8000.0, LofiMix: 0.6,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.2, SidechainAmount: 0.0,
            DelayTime: 2.0, DelayFeedback: 0.35, DelayMix: 0.25, ReverbSize: 0.8, ReverbDamping: 0.4, ReverbMix: 0.5,
        ],
    },
    // Grainy Pad: a very narrow pulse (bright, thin) doubled by a square, a 1.2 kHz
    // lowpass opening over 1.5 s, and 6-bit / quarter-rate crushing for digital grain.
    Preset {
        name: "Grainy Pad",
        category: Category::LofiPads,
        tags: &[
            "lofi", "pad", "grainy", "bitcrush", "digital", "dark", "texture",
        ],
        values: values![
            Osc1Wave: 2.0, Osc1Pulse: 0.1, Osc1Mix: 0.6, Osc1Unison: 2.0, Osc1Spread: 8.0,
            Osc2Wave: 2.0, Osc2Pulse: 0.5, Osc2Detune: 5.0, Osc2Mix: 0.4,
            SubLevel: 0.2, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 1200.0, FilterResonance: 0.15, FilterEnvAmount: 0.3,
            FilterAttack: 1.5, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.5, FilterKeyTrack: 0.3,
            AmpAttack: 1.0, AmpDecay: 1.0, AmpSustain: 0.9, AmpRelease: 2.0,
            LfoTarget: 1.0, LfoRate: 0.2, LfoDepth: 0.1, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 0.6,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 6.0, LofiRate: 4.0, LofiWobble: 0.2, LofiNoise: 0.2, LofiTone: 5000.0, LofiMix: 0.7,
            ChorusRate: 0.4, ChorusDepth: 0.4, ChorusMix: 0.4, SidechainAmount: 0.0,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.15, ReverbSize: 0.8, ReverbDamping: 0.5, ReverbMix: 0.45,
        ],
    },
    // Sunset Drive: four spread saws over a sub, a 1.6 kHz lowpass swaying slowly, a
    // 60 % chorus, light lo-fi and a 30 % sidechain pump for lo-fi hip hop drums.
    Preset {
        name: "Sunset Drive",
        category: Category::LofiPads,
        tags: &[
            "lofi",
            "pad",
            "warm",
            "sunset",
            "synthwave",
            "sidechain",
            "hip hop",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.6, Osc1Unison: 4.0, Osc1Spread: 15.0,
            Osc2Wave: 3.0, Osc2Detune: -6.0, Osc2Mix: 0.3,
            SubLevel: 0.4, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 1600.0, FilterResonance: 0.1, FilterEnvAmount: 0.2,
            FilterAttack: 0.6, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.5, FilterKeyTrack: 0.3,
            AmpAttack: 0.6, AmpDecay: 1.0, AmpSustain: 0.9, AmpRelease: 2.5,
            LfoTarget: 1.0, LfoRate: 0.12, LfoDepth: 0.25, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 0.6,
            DriveAmount: 0.05, DriveMix: 0.1,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.3, LofiNoise: 0.25, LofiTone: 6000.0, LofiMix: 0.6,
            ChorusRate: 0.4, ChorusDepth: 0.4, ChorusMix: 0.6, SidechainAmount: 0.3, SidechainRelease: 0.4,
            DelayTime: 2.0, DelayFeedback: 0.35, DelayMix: 0.2, ReverbSize: 0.8, ReverbDamping: 0.5, ReverbMix: 0.45,
        ],
    },
    // Wobble Pad: triangle + saw through a 1 kHz lowpass with the tape wobble at full
    // depth and the lo-fi fully wet (12 bits, 4 kHz cut): seasick warmth.
    Preset {
        name: "Wobble Pad",
        category: Category::LofiPads,
        tags: &["lofi", "pad", "wobble", "tape", "warm", "seasick", "dark"],
        values: values![
            Osc1Wave: 3.0, Osc1Mix: 0.7, Osc1Unison: 1.0,
            Osc2Wave: 1.0, Osc2Detune: 8.0, Osc2Mix: 0.3,
            SubLevel: 0.2, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 1000.0, FilterResonance: 0.15, FilterEnvAmount: 0.2,
            FilterAttack: 0.8, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.0, FilterKeyTrack: 0.3,
            AmpAttack: 0.8, AmpDecay: 1.0, AmpSustain: 0.9, AmpRelease: 2.0,
            LfoTarget: 1.0, LfoRate: 0.2, LfoDepth: 0.1, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.6,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 1.0, LofiNoise: 0.3, LofiTone: 4000.0, LofiMix: 1.0,
            ChorusRate: 0.3, ChorusDepth: 0.3, ChorusMix: 0.3, SidechainAmount: 0.0,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.1, ReverbSize: 0.8, ReverbDamping: 0.5, ReverbMix: 0.4,
        ],
    },
    // ----- EDM Leads -----------------------------------------------------------------------
    // Festival Saw: the supersaw lead: seven unison saws at 25 cents, a second saw layer
    // and a touch of sub, lowpass nearly open (9 kHz), tight envelope, chorus for width,
    // a 50 % sidechain pump, eighth-note delay and a hall.
    Preset {
        name: "Festival Saw",
        category: Category::EdmLeads,
        tags: &[
            "edm",
            "lead",
            "supersaw",
            "bright",
            "festival",
            "big room",
            "sidechain",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.8, Osc1Unison: 7.0, Osc1Spread: 25.0,
            Osc2Wave: 1.0, Osc2Detune: -12.0, Osc2Mix: 0.3,
            SubLevel: 0.15, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 9000.0, FilterResonance: 0.05, FilterEnvAmount: 0.2,
            FilterAttack: 0.005, FilterDecay: 0.3, FilterSustain: 0.5, FilterRelease: 0.3, FilterKeyTrack: 0.5,
            AmpAttack: 0.01, AmpDecay: 0.3, AmpSustain: 0.9, AmpRelease: 0.4,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.02,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.6,
            DriveAmount: 0.25, DriveMix: 0.3,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.6, ChorusDepth: 0.3, ChorusMix: 0.4, SidechainAmount: 0.5, SidechainRelease: 0.3,
            DelayTime: 1.0, DelayFeedback: 0.4, DelayMix: 0.25, ReverbSize: 0.7, ReverbDamping: 0.4, ReverbMix: 0.3,
        ],
    },
    // Supersaw Lead: wider (35 cents) and brighter (12 kHz) than Festival Saw, no sub, a
    // sharper 1-cent-flat second saw, more chorus and a slightly softer pump.
    Preset {
        name: "Supersaw Lead",
        category: Category::EdmLeads,
        tags: &[
            "edm",
            "lead",
            "supersaw",
            "bright",
            "wide",
            "trance",
            "sidechain",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.85, Osc1Unison: 7.0, Osc1Spread: 35.0,
            Osc2Wave: 1.0, Osc2Detune: 7.0, Osc2Mix: 0.25,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 12000.0, FilterResonance: 0.0, FilterEnvAmount: 0.1,
            FilterAttack: 0.005, FilterDecay: 0.3, FilterSustain: 0.5, FilterRelease: 0.3, FilterKeyTrack: 0.5,
            AmpAttack: 0.015, AmpDecay: 0.3, AmpSustain: 0.9, AmpRelease: 0.5,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.03,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.6,
            DriveAmount: 0.2, DriveMix: 0.25,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.8, ChorusDepth: 0.4, ChorusMix: 0.6, SidechainAmount: 0.4, SidechainRelease: 0.3,
            DelayTime: 1.0, DelayFeedback: 0.45, DelayMix: 0.3, ReverbSize: 0.75, ReverbDamping: 0.4, ReverbMix: 0.35,
        ],
    },
    // Laser Lead: a narrow pulse with a saw, a snappy resonant filter envelope, 6 Hz
    // vibrato, glide, drive and a feeding-back eighth delay: a cutting mono-style lead.
    Preset {
        name: "Laser Lead",
        category: Category::EdmLeads,
        tags: &[
            "edm", "lead", "bright", "laser", "pulse", "vibrato", "electro",
        ],
        values: values![
            Osc1Wave: 2.0, Osc1Pulse: 0.25, Osc1Mix: 0.8, Osc1Unison: 1.0,
            Osc2Wave: 1.0, Osc2Detune: 5.0, Osc2Mix: 0.4,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 4000.0, FilterResonance: 0.4, FilterEnvAmount: 0.6,
            FilterAttack: 0.001, FilterDecay: 0.15, FilterSustain: 0.3, FilterRelease: 0.2, FilterKeyTrack: 0.5,
            AmpAttack: 0.003, AmpDecay: 0.2, AmpSustain: 0.85, AmpRelease: 0.25,
            LfoTarget: 0.0, LfoRate: 6.0, LfoDepth: 0.12, Glide: 0.06,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.4, VelToAmp: 0.8,
            DriveAmount: 0.4, DriveMix: 0.5,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.15, SidechainAmount: 0.3, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.45, DelayMix: 0.3, ReverbSize: 0.6, ReverbDamping: 0.5, ReverbMix: 0.25,
        ],
    },
    // Hoover: five saws spread 40 cents plus a square an octave-ish down, the pitch
    // envelope sweeping UP from -12 semitones over a quarter second (the rave "hoover"
    // scoop), drive and chorus.
    Preset {
        name: "Hoover",
        category: Category::EdmLeads,
        tags: &[
            "edm",
            "lead",
            "hoover",
            "rave",
            "hardcore",
            "wide",
            "aggressive",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.7, Osc1Unison: 5.0, Osc1Spread: 40.0,
            Osc2Wave: 2.0, Osc2Pulse: 0.5, Osc2Detune: -12.0, Osc2Mix: 0.5,
            SubLevel: 0.2, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 3000.0, FilterResonance: 0.2, FilterEnvAmount: 0.3,
            FilterAttack: 0.01, FilterDecay: 0.4, FilterSustain: 0.6, FilterRelease: 0.3, FilterKeyTrack: 0.4,
            AmpAttack: 0.01, AmpDecay: 0.4, AmpSustain: 0.8, AmpRelease: 0.3,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: -12.0, PitchEnvDecay: 0.25, VelToCutoff: 0.3, VelToAmp: 0.7,
            DriveAmount: 0.5, DriveMix: 0.6,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.7, ChorusDepth: 0.4, ChorusMix: 0.5, SidechainAmount: 0.3, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.15, ReverbSize: 0.65, ReverbDamping: 0.4, ReverbMix: 0.3,
        ],
    },
    // Future Bass Chord: supersaw + pulse under a 2.5 kHz lowpass with a strong filter
    // envelope, a heavy 70 % sidechain pump, a 50 % chorus and drive: play chords.
    Preset {
        name: "Future Bass Chord",
        category: Category::EdmLeads,
        tags: &[
            "edm",
            "lead",
            "chord",
            "future bass",
            "sidechain",
            "bright",
            "wide",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.7, Osc1Unison: 7.0, Osc1Spread: 20.0,
            Osc2Wave: 2.0, Osc2Pulse: 0.35, Osc2Detune: 4.0, Osc2Mix: 0.3,
            SubLevel: 0.1, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 2500.0, FilterResonance: 0.15, FilterEnvAmount: 0.6,
            FilterAttack: 0.005, FilterDecay: 0.3, FilterSustain: 0.5, FilterRelease: 0.4, FilterKeyTrack: 0.4,
            AmpAttack: 0.02, AmpDecay: 0.5, AmpSustain: 0.7, AmpRelease: 0.5,
            LfoTarget: 1.0, LfoRate: 0.35, LfoDepth: 0.2, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 0.6,
            DriveAmount: 0.3, DriveMix: 0.4,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.6, ChorusDepth: 0.4, ChorusMix: 0.5, SidechainAmount: 0.7, SidechainRelease: 0.35,
            DelayTime: 1.0, DelayFeedback: 0.35, DelayMix: 0.2, ReverbSize: 0.7, ReverbDamping: 0.4, ReverbMix: 0.35,
        ],
    },
    // Trance Lead: three spread saws with a detuned square, a bright 6 kHz lowpass, a
    // sixteenth-note delay with 60 % feedback, vibrato, glide and a 40 % pump.
    Preset {
        name: "Trance Lead",
        category: Category::EdmLeads,
        tags: &[
            "edm",
            "lead",
            "trance",
            "bright",
            "delay",
            "uplifting",
            "sidechain",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.7, Osc1Unison: 3.0, Osc1Spread: 15.0,
            Osc2Wave: 2.0, Osc2Pulse: 0.5, Osc2Detune: -7.0, Osc2Mix: 0.5,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 6000.0, FilterResonance: 0.1, FilterEnvAmount: 0.3,
            FilterAttack: 0.005, FilterDecay: 0.25, FilterSustain: 0.4, FilterRelease: 0.3, FilterKeyTrack: 0.5,
            AmpAttack: 0.02, AmpDecay: 0.2, AmpSustain: 0.85, AmpRelease: 0.35,
            LfoTarget: 0.0, LfoRate: 5.5, LfoDepth: 0.1, Glide: 0.04,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 0.7,
            DriveAmount: 0.35, DriveMix: 0.45,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.3, ChorusMix: 0.3, SidechainAmount: 0.4, SidechainRelease: 0.3,
            DelayTime: 0.0, DelayFeedback: 0.6, DelayMix: 0.35, ReverbSize: 0.7, ReverbDamping: 0.4, ReverbMix: 0.3,
        ],
    },
    // Square Lead: a single square (chiptune / early Zelda), no filter envelope, 6.5 Hz
    // vibrato, a little glide, full sustain and a quick release; slapback and light drive.
    Preset {
        name: "Square Lead",
        category: Category::EdmLeads,
        tags: &[
            "edm", "lead", "square", "chiptune", "8-bit", "retro", "clean",
        ],
        values: values![
            Osc1Wave: 2.0, Osc1Pulse: 0.5, Osc1Mix: 1.0, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 8000.0, FilterResonance: 0.0, FilterEnvAmount: 0.0,
            FilterAttack: 0.005, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.5,
            AmpAttack: 0.005, AmpDecay: 0.1, AmpSustain: 1.0, AmpRelease: 0.15,
            LfoTarget: 0.0, LfoRate: 6.5, LfoDepth: 0.15, Glide: 0.05,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.0, VelToAmp: 0.5,
            DriveAmount: 0.2, DriveMix: 0.3,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.35, DelayMix: 0.25, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.2,
        ],
    },
    // FM Bell Lead: sine carrier with a 2:1 modulator at a high index (bright, bell-like
    // harmonics), a slightly sharp second sine, a decaying filter envelope, quarter-note
    // delay, hall and a 30 % pump.
    Preset {
        name: "FM Bell Lead",
        category: Category::EdmLeads,
        tags: &["edm", "lead", "fm", "bell", "bright", "melodic", "dubstep"],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 0.9, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 8.0, Osc2Mix: 0.3,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.6, FmRatio: 2.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 9000.0, FilterResonance: 0.1, FilterEnvAmount: 0.5,
            FilterAttack: 0.001, FilterDecay: 0.4, FilterSustain: 0.3, FilterRelease: 0.5, FilterKeyTrack: 0.5,
            AmpAttack: 0.002, AmpDecay: 0.6, AmpSustain: 0.5, AmpRelease: 0.6,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.4, VelToAmp: 0.8,
            DriveAmount: 0.1, DriveMix: 0.2,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.6, ChorusDepth: 0.3, ChorusMix: 0.3, SidechainAmount: 0.3, SidechainRelease: 0.3,
            DelayTime: 2.0, DelayFeedback: 0.45, DelayMix: 0.3, ReverbSize: 0.75, ReverbDamping: 0.4, ReverbMix: 0.35,
        ],
    },
    // Screech Lead: a very narrow pulse in unison with a detuned saw, a resonant lowpass
    // wobbled hard at 8 Hz, heavy drive: a dubstep / electro screech.
    Preset {
        name: "Screech Lead",
        category: Category::EdmLeads,
        tags: &[
            "edm",
            "lead",
            "screech",
            "aggressive",
            "dubstep",
            "electro",
            "bright",
        ],
        values: values![
            Osc1Wave: 2.0, Osc1Pulse: 0.15, Osc1Mix: 0.7, Osc1Unison: 3.0, Osc1Spread: 30.0,
            Osc2Wave: 1.0, Osc2Detune: -15.0, Osc2Mix: 0.4,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 5000.0, FilterResonance: 0.7, FilterEnvAmount: 0.4,
            FilterAttack: 0.005, FilterDecay: 0.2, FilterSustain: 0.5, FilterRelease: 0.2, FilterKeyTrack: 0.4,
            AmpAttack: 0.005, AmpDecay: 0.2, AmpSustain: 0.9, AmpRelease: 0.2,
            LfoTarget: 1.0, LfoRate: 8.0, LfoDepth: 0.6, Glide: 0.02,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 0.6,
            DriveAmount: 0.8, DriveMix: 0.9,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.2, SidechainAmount: 0.3, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.15, ReverbSize: 0.6, ReverbDamping: 0.5, ReverbMix: 0.2,
        ],
    },
    // Pluck Lead: a progressive-house lead with a deep filter envelope (1.5 kHz + 3.6
    // octaves closing over half a second), 40 % sustain, chorus, delay and a 50 % pump.
    Preset {
        name: "Pluck Lead",
        category: Category::EdmLeads,
        tags: &[
            "edm",
            "lead",
            "pluck",
            "progressive",
            "house",
            "sidechain",
            "melodic",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.7, Osc1Unison: 3.0, Osc1Spread: 12.0,
            Osc2Wave: 2.0, Osc2Pulse: 0.5, Osc2Detune: 3.0, Osc2Mix: 0.3,
            SubLevel: 0.1, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 1500.0, FilterResonance: 0.25, FilterEnvAmount: 0.9,
            FilterAttack: 0.001, FilterDecay: 0.5, FilterSustain: 0.2, FilterRelease: 0.4, FilterKeyTrack: 0.5,
            AmpAttack: 0.003, AmpDecay: 0.9, AmpSustain: 0.4, AmpRelease: 0.5,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.5, VelToAmp: 0.8,
            DriveAmount: 0.2, DriveMix: 0.3,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.6, ChorusDepth: 0.3, ChorusMix: 0.3, SidechainAmount: 0.5, SidechainRelease: 0.3,
            DelayTime: 1.0, DelayFeedback: 0.45, DelayMix: 0.3, ReverbSize: 0.7, ReverbDamping: 0.4, ReverbMix: 0.3,
        ],
    },
    // Big Room Lead: seven saws at 30 cents, a square an octave-ish down and a sub for
    // weight, 7 kHz lowpass, hard drive and a 60 % pump: the drop lead.
    Preset {
        name: "Big Room Lead",
        category: Category::EdmLeads,
        tags: &[
            "edm",
            "lead",
            "big room",
            "festival",
            "fat",
            "wide",
            "sidechain",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.9, Osc1Unison: 7.0, Osc1Spread: 30.0,
            Osc2Wave: 2.0, Osc2Pulse: 0.5, Osc2Detune: -12.0, Osc2Mix: 0.4,
            SubLevel: 0.3, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 7000.0, FilterResonance: 0.1, FilterEnvAmount: 0.2,
            FilterAttack: 0.005, FilterDecay: 0.3, FilterSustain: 0.6, FilterRelease: 0.3, FilterKeyTrack: 0.4,
            AmpAttack: 0.01, AmpDecay: 0.3, AmpSustain: 0.9, AmpRelease: 0.3,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.01,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.5,
            DriveAmount: 0.5, DriveMix: 0.6,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.3, ChorusMix: 0.3, SidechainAmount: 0.6, SidechainRelease: 0.3,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.15, ReverbSize: 0.75, ReverbDamping: 0.4, ReverbMix: 0.35,
        ],
    },
    // ----- EDM Bass ------------------------------------------------------------------------
    // Trap 808: a sine with the sub an octave down, a +12 semitone pitch drop over 80 ms
    // (the 808 click), a closed lowpass that keeps everything under 200 Hz on a low note,
    // 808 saturation from the drive, glide for slides, no sends.
    Preset {
        name: "Trap 808",
        category: Category::EdmBass,
        tags: &["edm", "bass", "808", "trap", "sub", "hip hop", "dark"],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 1.0, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 0.6, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 300.0, FilterResonance: 0.1, FilterEnvAmount: 0.3,
            FilterAttack: 0.001, FilterDecay: 0.15, FilterSustain: 0.0, FilterRelease: 0.2, FilterKeyTrack: 0.3,
            AmpAttack: 0.002, AmpDecay: 0.6, AmpSustain: 0.6, AmpRelease: 0.4,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.04,
            PitchEnvAmount: 12.0, PitchEnvDecay: 0.08, VelToCutoff: 0.2, VelToAmp: 0.7,
            DriveAmount: 0.5, DriveMix: 0.5,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.0, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.0,
        ],
    },
    // 808 Sub: cleaner and longer than Trap 808: more sub, a faster 50 ms drop, a 250 Hz
    // lowpass, a 1.2 s decay into 40 % sustain, gentle drive.
    Preset {
        name: "808 Sub",
        category: Category::EdmBass,
        tags: &["edm", "bass", "808", "sub", "trap", "clean", "long"],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 1.0, Osc1Unison: 1.0,
            Osc2Wave: 3.0, Osc2Detune: 0.0, Osc2Mix: 0.1,
            SubLevel: 0.8, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 250.0, FilterResonance: 0.05, FilterEnvAmount: 0.2,
            FilterAttack: 0.001, FilterDecay: 0.1, FilterSustain: 0.0, FilterRelease: 0.2, FilterKeyTrack: 0.3,
            AmpAttack: 0.002, AmpDecay: 1.2, AmpSustain: 0.4, AmpRelease: 0.5,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.06,
            PitchEnvAmount: 12.0, PitchEnvDecay: 0.05, VelToCutoff: 0.1, VelToAmp: 0.6,
            DriveAmount: 0.3, DriveMix: 0.4,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.0, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.0,
        ],
    },
    // Reese Bass: two unison saws 18 cents apart against a third saw 14 cents flat (the
    // slow beating), a 600 Hz lowpass, drive, a little chorus and a 40 % pump: DnB reese.
    Preset {
        name: "Reese Bass",
        category: Category::EdmBass,
        tags: &[
            "edm",
            "bass",
            "reese",
            "dnb",
            "dark",
            "detuned",
            "sidechain",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.7, Osc1Unison: 2.0, Osc1Spread: 18.0,
            Osc2Wave: 1.0, Osc2Detune: -14.0, Osc2Mix: 0.6,
            SubLevel: 0.2, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 600.0, FilterResonance: 0.2, FilterEnvAmount: 0.1,
            FilterAttack: 0.01, FilterDecay: 0.3, FilterSustain: 0.5, FilterRelease: 0.2, FilterKeyTrack: 0.3,
            AmpAttack: 0.01, AmpDecay: 0.3, AmpSustain: 0.9, AmpRelease: 0.2,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.02,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.5,
            DriveAmount: 0.4, DriveMix: 0.5,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.3, ChorusDepth: 0.2, ChorusMix: 0.3, SidechainAmount: 0.4, SidechainRelease: 0.2,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.0, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.05,
        ],
    },
    // Neuro Growl: a narrow pulse against a detuned saw with a sub, a resonant BANDPASS
    // at 900 Hz wobbled at 3.5 Hz, heavy drive and 8-bit / half-rate grit: neurofunk.
    Preset {
        name: "Neuro Growl",
        category: Category::EdmBass,
        tags: &[
            "edm",
            "bass",
            "neuro",
            "growl",
            "dnb",
            "dubstep",
            "aggressive",
            "dark",
        ],
        values: values![
            Osc1Wave: 2.0, Osc1Pulse: 0.2, Osc1Mix: 0.7, Osc1Unison: 1.0,
            Osc2Wave: 1.0, Osc2Detune: -10.0, Osc2Mix: 0.5,
            SubLevel: 0.3, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 1.0, FilterCutoff: 900.0, FilterResonance: 0.6, FilterEnvAmount: 0.3,
            FilterAttack: 0.005, FilterDecay: 0.2, FilterSustain: 0.5, FilterRelease: 0.2, FilterKeyTrack: 0.3,
            AmpAttack: 0.005, AmpDecay: 0.2, AmpSustain: 1.0, AmpRelease: 0.15,
            LfoTarget: 1.0, LfoRate: 3.5, LfoDepth: 0.5, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.4,
            DriveAmount: 0.8, DriveMix: 0.9,
            LofiBits: 8.0, LofiRate: 2.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 12000.0, LofiMix: 0.4,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.2, SidechainAmount: 0.4, SidechainRelease: 0.2,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.05, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.05,
        ],
    },
    // Wobble Bass: saw + square an octave-ish down over a sub, a resonant 800 Hz lowpass
    // swept +-1.6 octaves by a 2 Hz LFO (eighth-note wobble at 120 BPM), drive.
    Preset {
        name: "Wobble Bass",
        category: Category::EdmBass,
        tags: &[
            "edm",
            "bass",
            "wobble",
            "dubstep",
            "lfo",
            "dark",
            "aggressive",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.8, Osc1Unison: 1.0,
            Osc2Wave: 2.0, Osc2Pulse: 0.5, Osc2Detune: -12.0, Osc2Mix: 0.5,
            SubLevel: 0.4, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 800.0, FilterResonance: 0.5, FilterEnvAmount: 0.0,
            FilterAttack: 0.005, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.3,
            AmpAttack: 0.005, AmpDecay: 0.2, AmpSustain: 1.0, AmpRelease: 0.15,
            LfoTarget: 1.0, LfoRate: 2.0, LfoDepth: 0.8, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.1, VelToAmp: 0.4,
            DriveAmount: 0.6, DriveMix: 0.7,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.3, SidechainRelease: 0.2,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.0, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.05,
        ],
    },
    // Deep House Sub: sine + a little triangle over a sub, a 220 Hz lowpass, a tiny
    // 3-semitone click, soft drive and a strong 60 % pump against the kick.
    Preset {
        name: "Deep House Sub",
        category: Category::EdmBass,
        tags: &["edm", "bass", "sub", "house", "deep", "clean", "sidechain"],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 1.0, Osc1Unison: 1.0,
            Osc2Wave: 3.0, Osc2Detune: 0.0, Osc2Mix: 0.2,
            SubLevel: 0.5, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 220.0, FilterResonance: 0.05, FilterEnvAmount: 0.2,
            FilterAttack: 0.001, FilterDecay: 0.15, FilterSustain: 0.0, FilterRelease: 0.2, FilterKeyTrack: 0.3,
            AmpAttack: 0.005, AmpDecay: 0.3, AmpSustain: 0.7, AmpRelease: 0.25,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 3.0, PitchEnvDecay: 0.03, VelToCutoff: 0.1, VelToAmp: 0.5,
            DriveAmount: 0.15, DriveMix: 0.2,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.6, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.0, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.0,
        ],
    },
    // Future House Bass: three unison pulses with a saw a fifth-ish sharp, a 500 Hz
    // lowpass snapped by a deep fast filter envelope, 30 % sustain, drive and a 50 % pump.
    Preset {
        name: "Future House Bass",
        category: Category::EdmBass,
        tags: &[
            "edm",
            "bass",
            "future house",
            "house",
            "pluck",
            "punchy",
            "sidechain",
        ],
        values: values![
            Osc1Wave: 2.0, Osc1Pulse: 0.4, Osc1Mix: 0.7, Osc1Unison: 3.0, Osc1Spread: 10.0,
            Osc2Wave: 1.0, Osc2Detune: 7.0, Osc2Mix: 0.5,
            SubLevel: 0.2, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 500.0, FilterResonance: 0.35, FilterEnvAmount: 0.8,
            FilterAttack: 0.001, FilterDecay: 0.25, FilterSustain: 0.1, FilterRelease: 0.2, FilterKeyTrack: 0.3,
            AmpAttack: 0.002, AmpDecay: 0.35, AmpSustain: 0.3, AmpRelease: 0.15,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.4, VelToAmp: 0.6,
            DriveAmount: 0.5, DriveMix: 0.6,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.5, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.05, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.05,
        ],
    },
    // Fat Mono: saw + square 8 cents flat over a strong sub, a 450 Hz lowpass with a
    // medium envelope, glide for legato lines, moderate drive: a Moog-style synth bass.
    Preset {
        name: "Fat Mono",
        category: Category::EdmBass,
        tags: &["edm", "bass", "moog", "analog", "fat", "warm", "mono"],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.8, Osc1Unison: 1.0,
            Osc2Wave: 2.0, Osc2Pulse: 0.5, Osc2Detune: -8.0, Osc2Mix: 0.6,
            SubLevel: 0.5, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 450.0, FilterResonance: 0.3, FilterEnvAmount: 0.6,
            FilterAttack: 0.001, FilterDecay: 0.3, FilterSustain: 0.2, FilterRelease: 0.2, FilterKeyTrack: 0.3,
            AmpAttack: 0.003, AmpDecay: 0.3, AmpSustain: 0.6, AmpRelease: 0.15,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.06,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.5, VelToAmp: 0.7,
            DriveAmount: 0.3, DriveMix: 0.4,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.0, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.05,
        ],
    },
    // FM Bass: sine carrier with a 1:1 modulator at a high index (buzzy harmonics that
    // the filter envelope then tames), a sub underneath, drive and a 40 % pump.
    Preset {
        name: "FM Bass",
        category: Category::EdmBass,
        tags: &[
            "edm",
            "bass",
            "fm",
            "buzzy",
            "garage",
            "punchy",
            "sidechain",
        ],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 0.9, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 0.3, NoiseLevel: 0.0, FmAmount: 0.55, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 1500.0, FilterResonance: 0.15, FilterEnvAmount: 0.7,
            FilterAttack: 0.001, FilterDecay: 0.2, FilterSustain: 0.1, FilterRelease: 0.2, FilterKeyTrack: 0.3,
            AmpAttack: 0.002, AmpDecay: 0.4, AmpSustain: 0.4, AmpRelease: 0.2,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.5, VelToAmp: 0.7,
            DriveAmount: 0.3, DriveMix: 0.4,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.4, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.0, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.05,
        ],
    },
    // Dub Sub: sine + a heavy sub under a 150 Hz lowpass, slow 20 ms attack, long glide,
    // no drive, a light quarter-note echo: dub / dubstep sub.
    Preset {
        name: "Dub Sub",
        category: Category::EdmBass,
        tags: &["edm", "bass", "sub", "dub", "dubstep", "clean", "deep"],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 1.0, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 0.7, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 150.0, FilterResonance: 0.0, FilterEnvAmount: 0.1,
            FilterAttack: 0.01, FilterDecay: 0.2, FilterSustain: 0.5, FilterRelease: 0.3, FilterKeyTrack: 0.3,
            AmpAttack: 0.02, AmpDecay: 0.5, AmpSustain: 0.9, AmpRelease: 0.4,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.1,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.0, VelToAmp: 0.4,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.3, SidechainRelease: 0.3,
            DelayTime: 2.0, DelayFeedback: 0.5, DelayMix: 0.15, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.0,
        ],
    },
    // Tape 303: an acid saw with screaming resonance and a deep fast filter envelope,
    // glide and drive, run through 10-bit / half-rate lo-fi with tape wobble and dust.
    Preset {
        name: "Tape 303",
        category: Category::EdmBass,
        tags: &[
            "edm", "bass", "acid", "303", "techno", "lofi", "tape", "squelch",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 1.0, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 400.0, FilterResonance: 0.8, FilterEnvAmount: 0.85,
            FilterAttack: 0.001, FilterDecay: 0.2, FilterSustain: 0.05, FilterRelease: 0.15, FilterKeyTrack: 0.3,
            AmpAttack: 0.002, AmpDecay: 0.25, AmpSustain: 0.1, AmpRelease: 0.1,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.1,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.6, VelToAmp: 0.8,
            DriveAmount: 0.6, DriveMix: 0.7,
            LofiBits: 10.0, LofiRate: 2.0, LofiWobble: 0.3, LofiNoise: 0.2, LofiTone: 7000.0, LofiMix: 0.5,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.2, SidechainRelease: 0.2,
            DelayTime: 0.0, DelayFeedback: 0.4, DelayMix: 0.2, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.1,
        ],
    },
    // Distorted 808: Trap 808 pushed through full drive and 8-bit crushing, a 500 Hz
    // lowpass to let the harmonics through, a longer 1 s decay: the blown-out 808.
    Preset {
        name: "Distorted 808",
        category: Category::EdmBass,
        tags: &[
            "edm",
            "bass",
            "808",
            "trap",
            "distorted",
            "aggressive",
            "dark",
        ],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 1.0, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 0.5, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 500.0, FilterResonance: 0.1, FilterEnvAmount: 0.3,
            FilterAttack: 0.001, FilterDecay: 0.2, FilterSustain: 0.0, FilterRelease: 0.2, FilterKeyTrack: 0.3,
            AmpAttack: 0.002, AmpDecay: 1.0, AmpSustain: 0.5, AmpRelease: 0.3,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.05,
            PitchEnvAmount: 12.0, PitchEnvDecay: 0.08, VelToCutoff: 0.2, VelToAmp: 0.5,
            DriveAmount: 0.9, DriveMix: 1.0,
            LofiBits: 8.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.5,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.0, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.0,
        ],
    },
    // ----- EDM Plucks ----------------------------------------------------------------------
    // Glass Pluck: sine with a 3.5:1 FM modulator (glassy inharmonic partials), a sharp
    // triangle double, an 8 kHz lowpass snapping shut, no sustain, eighth delay, hall.
    Preset {
        name: "Glass Pluck",
        category: Category::EdmPlucks,
        tags: &[
            "edm",
            "pluck",
            "glass",
            "fm",
            "bright",
            "melodic",
            "sidechain",
        ],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 0.9, Osc1Unison: 1.0,
            Osc2Wave: 3.0, Osc2Detune: 4.0, Osc2Mix: 0.3,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.35, FmRatio: 3.5, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 8000.0, FilterResonance: 0.1, FilterEnvAmount: 0.6,
            FilterAttack: 0.001, FilterDecay: 0.25, FilterSustain: 0.0, FilterRelease: 0.3, FilterKeyTrack: 0.5,
            AmpAttack: 0.001, AmpDecay: 0.3, AmpSustain: 0.0, AmpRelease: 0.35,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.4, VelToAmp: 0.8,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.6, ChorusDepth: 0.3, ChorusMix: 0.2, SidechainAmount: 0.3, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.45, DelayMix: 0.35, ReverbSize: 0.75, ReverbDamping: 0.4, ReverbMix: 0.4,
        ],
    },
    // Progressive Pluck: three spread saws + a pulse, a 900 Hz lowpass driven 4 octaves up
    // by a full filter envelope closing in 0.3 s, drive, a feeding-back eighth delay.
    Preset {
        name: "Progressive Pluck",
        category: Category::EdmPlucks,
        tags: &[
            "edm",
            "pluck",
            "progressive",
            "house",
            "melodic",
            "sidechain",
            "delay",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.7, Osc1Unison: 3.0, Osc1Spread: 12.0,
            Osc2Wave: 2.0, Osc2Pulse: 0.4, Osc2Detune: 3.0, Osc2Mix: 0.3,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 900.0, FilterResonance: 0.3, FilterEnvAmount: 1.0,
            FilterAttack: 0.001, FilterDecay: 0.3, FilterSustain: 0.0, FilterRelease: 0.3, FilterKeyTrack: 0.5,
            AmpAttack: 0.001, AmpDecay: 0.35, AmpSustain: 0.0, AmpRelease: 0.3,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.5, VelToAmp: 0.8,
            DriveAmount: 0.2, DriveMix: 0.3,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.2, SidechainAmount: 0.5, SidechainRelease: 0.3,
            DelayTime: 1.0, DelayFeedback: 0.5, DelayMix: 0.35, ReverbSize: 0.7, ReverbDamping: 0.4, ReverbMix: 0.3,
        ],
    },
    // Trance Pluck: two detuned saws, a resonant 1.2 kHz lowpass with a deep envelope, a
    // sixteenth-note delay at 65 % feedback for the classic bouncing pluck.
    Preset {
        name: "Trance Pluck",
        category: Category::EdmPlucks,
        tags: &[
            "edm",
            "pluck",
            "trance",
            "bright",
            "delay",
            "uplifting",
            "sidechain",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.8, Osc1Unison: 1.0,
            Osc2Wave: 1.0, Osc2Detune: -10.0, Osc2Mix: 0.5,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 1200.0, FilterResonance: 0.4, FilterEnvAmount: 0.9,
            FilterAttack: 0.001, FilterDecay: 0.2, FilterSustain: 0.0, FilterRelease: 0.25, FilterKeyTrack: 0.5,
            AmpAttack: 0.001, AmpDecay: 0.25, AmpSustain: 0.0, AmpRelease: 0.25,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.5, VelToAmp: 0.8,
            DriveAmount: 0.25, DriveMix: 0.3,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.15, SidechainAmount: 0.4, SidechainRelease: 0.25,
            DelayTime: 0.0, DelayFeedback: 0.65, DelayMix: 0.4, ReverbSize: 0.6, ReverbDamping: 0.5, ReverbMix: 0.3,
        ],
    },
    // Pizzicato: a short Karplus-Strong string (damping 0.8) with a medium excitation, the
    // amp envelope falling to 30 % so it behaves like a plucked orchestral string.
    Preset {
        name: "Pizzicato",
        category: Category::EdmPlucks,
        tags: &[
            "edm",
            "pluck",
            "pizzicato",
            "strings",
            "orchestral",
            "short",
            "clean",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 1.0, Osc2Mix: 0.0, SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0,
            OscMode: 1.0, PluckDamping: 0.8, PluckBright: 0.6,
            FilterType: 0.0, FilterCutoff: 5000.0, FilterResonance: 0.1, FilterEnvAmount: 0.2,
            FilterAttack: 0.001, FilterDecay: 0.15, FilterSustain: 0.0, FilterRelease: 0.2, FilterKeyTrack: 0.5,
            AmpAttack: 0.001, AmpDecay: 0.3, AmpSustain: 0.3, AmpRelease: 0.3,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.4, VelToAmp: 1.0,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.15, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.2, ReverbSize: 0.7, ReverbDamping: 0.5, ReverbMix: 0.35,
        ],
    },
    // Kalimba: sine with a hint of 4:1 FM for the tine's overtone, a 2-semitone thumb
    // click, half-second decay, a quarter-note echo, hall, and a dusting of lo-fi.
    Preset {
        name: "Kalimba",
        category: Category::EdmPlucks,
        tags: &[
            "edm", "pluck", "kalimba", "mallet", "soft", "lofi", "melodic",
        ],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 1.0, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.15, FmRatio: 4.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 7000.0, FilterResonance: 0.1, FilterEnvAmount: 0.3,
            FilterAttack: 0.001, FilterDecay: 0.15, FilterSustain: 0.0, FilterRelease: 0.3, FilterKeyTrack: 0.6,
            AmpAttack: 0.001, AmpDecay: 0.5, AmpSustain: 0.0, AmpRelease: 0.5,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 2.0, PitchEnvDecay: 0.01, VelToCutoff: 0.4, VelToAmp: 1.0,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.1, LofiNoise: 0.1, LofiTone: 10000.0, LofiMix: 0.3,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.1, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 2.0, DelayFeedback: 0.4, DelayMix: 0.3, ReverbSize: 0.7, ReverbDamping: 0.4, ReverbMix: 0.4,
        ],
    },
    // Marimba: triangle with a sine double and a trace of 4:1 FM, a 5 kHz lowpass with a
    // quick envelope, 0.4 s decay, high key tracking, eighth echo.
    Preset {
        name: "Marimba",
        category: Category::EdmPlucks,
        tags: &[
            "edm",
            "pluck",
            "marimba",
            "mallet",
            "wood",
            "warm",
            "percussive",
        ],
        values: values![
            Osc1Wave: 3.0, Osc1Mix: 0.9, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.4,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.08, FmRatio: 4.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 5000.0, FilterResonance: 0.1, FilterEnvAmount: 0.5,
            FilterAttack: 0.001, FilterDecay: 0.2, FilterSustain: 0.0, FilterRelease: 0.3, FilterKeyTrack: 0.6,
            AmpAttack: 0.001, AmpDecay: 0.4, AmpSustain: 0.0, AmpRelease: 0.4,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.5, VelToAmp: 1.0,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.35, DelayMix: 0.2, ReverbSize: 0.6, ReverbDamping: 0.5, ReverbMix: 0.3,
        ],
    },
    // House Stab: three spread saws + a square an octave-ish down + a sub, a 1.5 kHz
    // lowpass with a fast deep envelope, no sustain, drive, chorus and a 60 % pump.
    Preset {
        name: "House Stab",
        category: Category::EdmPlucks,
        tags: &[
            "edm",
            "pluck",
            "stab",
            "house",
            "chord",
            "punchy",
            "sidechain",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.7, Osc1Unison: 3.0, Osc1Spread: 10.0,
            Osc2Wave: 2.0, Osc2Pulse: 0.5, Osc2Detune: -12.0, Osc2Mix: 0.5,
            SubLevel: 0.2, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 1500.0, FilterResonance: 0.2, FilterEnvAmount: 0.8,
            FilterAttack: 0.001, FilterDecay: 0.15, FilterSustain: 0.1, FilterRelease: 0.2, FilterKeyTrack: 0.4,
            AmpAttack: 0.001, AmpDecay: 0.2, AmpSustain: 0.0, AmpRelease: 0.2,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.4, VelToAmp: 0.8,
            DriveAmount: 0.4, DriveMix: 0.5,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.6, ChorusDepth: 0.3, ChorusMix: 0.3, SidechainAmount: 0.6, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.2, ReverbSize: 0.6, ReverbDamping: 0.5, ReverbMix: 0.25,
        ],
    },
    // Water Drop: a pure sine with a +10 semitone drip (50 ms pitch envelope), a quarter
    // second decay, a quarter-note echo at 50 % feedback and a long hall.
    Preset {
        name: "Water Drop",
        category: Category::EdmPlucks,
        tags: &[
            "edm", "pluck", "water", "drop", "sine", "fx", "melodic", "soft",
        ],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 1.0, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 9000.0, FilterResonance: 0.0, FilterEnvAmount: 0.0,
            FilterAttack: 0.001, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.5,
            AmpAttack: 0.001, AmpDecay: 0.25, AmpSustain: 0.0, AmpRelease: 0.3,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 10.0, PitchEnvDecay: 0.05, VelToCutoff: 0.0, VelToAmp: 1.0,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 2.0, DelayFeedback: 0.5, DelayMix: 0.4, ReverbSize: 0.8, ReverbDamping: 0.3, ReverbMix: 0.5,
        ],
    },
    // 8-Bit Pluck: a lone square crushed to 6 bits at a quarter of the sample rate (no
    // wobble, no noise), a fast filter snap, a sixteenth-note echo and a 30 % pump.
    Preset {
        name: "8-Bit Pluck",
        category: Category::EdmPlucks,
        tags: &[
            "edm", "pluck", "8-bit", "chiptune", "bitcrush", "retro", "bright",
        ],
        values: values![
            Osc1Wave: 2.0, Osc1Pulse: 0.5, Osc1Mix: 1.0, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 6000.0, FilterResonance: 0.1, FilterEnvAmount: 0.5,
            FilterAttack: 0.001, FilterDecay: 0.1, FilterSustain: 0.0, FilterRelease: 0.15, FilterKeyTrack: 0.5,
            AmpAttack: 0.001, AmpDecay: 0.15, AmpSustain: 0.0, AmpRelease: 0.1,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 0.7,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 6.0, LofiRate: 4.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 1.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.3, SidechainRelease: 0.25,
            DelayTime: 0.0, DelayFeedback: 0.5, DelayMix: 0.3, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.15,
        ],
    },
    // Harp Pluck: a long-ringing bright string with 80 % sustain and a 1 s release,
    // chorus, eighth delay and a large hall: the EDM harp / arp sound.
    Preset {
        name: "Harp Pluck",
        category: Category::EdmPlucks,
        tags: &[
            "edm",
            "pluck",
            "harp",
            "arp",
            "bright",
            "melodic",
            "sidechain",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 1.0, Osc2Mix: 0.0, SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0,
            OscMode: 1.0, PluckDamping: 0.25, PluckBright: 0.75,
            FilterType: 0.0, FilterCutoff: 7000.0, FilterResonance: 0.05, FilterEnvAmount: 0.0,
            FilterAttack: 0.001, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.5,
            AmpAttack: 0.001, AmpDecay: 0.5, AmpSustain: 0.8, AmpRelease: 1.0,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 1.0,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.3, ChorusMix: 0.3, SidechainAmount: 0.3, SidechainRelease: 0.3,
            DelayTime: 1.0, DelayFeedback: 0.4, DelayMix: 0.3, ReverbSize: 0.8, ReverbDamping: 0.4, ReverbMix: 0.45,
        ],
    },
    // Rain Pluck: five spread saws under a 700 Hz lowpass with a full envelope decaying
    // over 0.4 s, half-second decay, chorus, a quarter-note echo and a huge hall: melodic
    // dubstep / future bass arps.
    Preset {
        name: "Rain Pluck",
        category: Category::EdmPlucks,
        tags: &[
            "edm",
            "pluck",
            "melodic",
            "dubstep",
            "future bass",
            "wide",
            "sidechain",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.8, Osc1Unison: 5.0, Osc1Spread: 15.0,
            Osc2Wave: 1.0, Osc2Detune: -5.0, Osc2Mix: 0.2,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 700.0, FilterResonance: 0.3, FilterEnvAmount: 1.0,
            FilterAttack: 0.001, FilterDecay: 0.4, FilterSustain: 0.0, FilterRelease: 0.4, FilterKeyTrack: 0.5,
            AmpAttack: 0.001, AmpDecay: 0.5, AmpSustain: 0.0, AmpRelease: 0.4,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.5, VelToAmp: 0.8,
            DriveAmount: 0.1, DriveMix: 0.2,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.4, ChorusMix: 0.5, SidechainAmount: 0.5, SidechainRelease: 0.3,
            DelayTime: 2.0, DelayFeedback: 0.45, DelayMix: 0.35, ReverbSize: 0.85, ReverbDamping: 0.4, ReverbMix: 0.5,
        ],
    },
    // ----- EDM Pads ------------------------------------------------------------------------
    // Big Pad: seven spread saws with a second saw an octave-ish down and a sub, a 2.5 kHz
    // lowpass opening over a second, 1 s / 2 s envelope, chorus, a 50 % pump, hall + echo.
    Preset {
        name: "Big Pad",
        category: Category::EdmPads,
        tags: &["edm", "pad", "big", "wide", "supersaw", "sidechain", "warm"],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.6, Osc1Unison: 7.0, Osc1Spread: 25.0,
            Osc2Wave: 1.0, Osc2Detune: -12.0, Osc2Mix: 0.4,
            SubLevel: 0.25, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 2500.0, FilterResonance: 0.1, FilterEnvAmount: 0.2,
            FilterAttack: 1.0, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.5, FilterKeyTrack: 0.3,
            AmpAttack: 1.0, AmpDecay: 1.0, AmpSustain: 0.9, AmpRelease: 2.0,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.1, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.6,
            DriveAmount: 0.1, DriveMix: 0.2,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.4, ChorusMix: 0.5, SidechainAmount: 0.5, SidechainRelease: 0.4,
            DelayTime: 2.0, DelayFeedback: 0.4, DelayMix: 0.25, ReverbSize: 0.85, ReverbDamping: 0.4, ReverbMix: 0.5,
        ],
    },
    // Anthem Pad: wider (35 cents) and brighter (4 kHz) than Big Pad with a narrow pulse
    // layer, a faster 0.5 s attack, a 70 % chorus and a hard 70 % pump: the festival pad.
    Preset {
        name: "Anthem Pad",
        category: Category::EdmPads,
        tags: &[
            "edm",
            "pad",
            "anthem",
            "festival",
            "bright",
            "wide",
            "sidechain",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.7, Osc1Unison: 7.0, Osc1Spread: 35.0,
            Osc2Wave: 2.0, Osc2Pulse: 0.3, Osc2Detune: 5.0, Osc2Mix: 0.3,
            SubLevel: 0.15, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 4000.0, FilterResonance: 0.05, FilterEnvAmount: 0.2,
            FilterAttack: 0.5, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.0, FilterKeyTrack: 0.3,
            AmpAttack: 0.5, AmpDecay: 1.0, AmpSustain: 0.9, AmpRelease: 1.5,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.5,
            DriveAmount: 0.2, DriveMix: 0.3,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.7, ChorusDepth: 0.4, ChorusMix: 0.7, SidechainAmount: 0.7, SidechainRelease: 0.35,
            DelayTime: 1.0, DelayFeedback: 0.4, DelayMix: 0.2, ReverbSize: 0.8, ReverbDamping: 0.4, ReverbMix: 0.45,
        ],
    },
    // Analog Strings: five spread saws plus a sharp saw, a 3 kHz lowpass swaying at
    // 0.3 Hz, a slow deep chorus (the string-machine ensemble), 0.8 s / 1.8 s envelope.
    Preset {
        name: "Analog Strings",
        category: Category::EdmPads,
        tags: &[
            "edm", "pad", "strings", "analog", "ensemble", "warm", "chorus",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.6, Osc1Unison: 5.0, Osc1Spread: 15.0,
            Osc2Wave: 1.0, Osc2Detune: 9.0, Osc2Mix: 0.5,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 3000.0, FilterResonance: 0.1, FilterEnvAmount: 0.15,
            FilterAttack: 0.5, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.0, FilterKeyTrack: 0.3,
            AmpAttack: 0.8, AmpDecay: 0.5, AmpSustain: 0.9, AmpRelease: 1.8,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.15, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 0.7,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.5, ChorusMix: 0.6, SidechainAmount: 0.2, SidechainRelease: 0.3,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.15, ReverbSize: 0.8, ReverbDamping: 0.4, ReverbMix: 0.45,
        ],
    },
    // Deep Pad: triangles in unison with a sine and a strong sub, a 900 Hz lowpass with a
    // slow 0.2 Hz sway, 0.6 s / 2 s envelope and a 60 % pump: deep house chords.
    Preset {
        name: "Deep Pad",
        category: Category::EdmPads,
        tags: &["edm", "pad", "deep", "house", "dark", "warm", "sidechain"],
        values: values![
            Osc1Wave: 3.0, Osc1Mix: 0.7, Osc1Unison: 3.0, Osc1Spread: 8.0,
            Osc2Wave: 0.0, Osc2Detune: 4.0, Osc2Mix: 0.5,
            SubLevel: 0.4, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 900.0, FilterResonance: 0.2, FilterEnvAmount: 0.1,
            FilterAttack: 0.3, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.0, FilterKeyTrack: 0.3,
            AmpAttack: 0.6, AmpDecay: 1.0, AmpSustain: 0.9, AmpRelease: 2.0,
            LfoTarget: 1.0, LfoRate: 0.2, LfoDepth: 0.25, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.6,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.4, ChorusDepth: 0.3, ChorusMix: 0.4, SidechainAmount: 0.6, SidechainRelease: 0.3,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.15, ReverbSize: 0.8, ReverbDamping: 0.5, ReverbMix: 0.4,
        ],
    },
    // Trance Gate: five spread saws through a resonant 1.5 kHz lowpass chopped by an 8 Hz
    // full-depth cutoff LFO (a sixteenth-note gate at 120 BPM), sixteenth echo, 50 % pump.
    Preset {
        name: "Trance Gate",
        category: Category::EdmPads,
        tags: &[
            "edm",
            "pad",
            "gate",
            "trance",
            "rhythmic",
            "bright",
            "sidechain",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.7, Osc1Unison: 5.0, Osc1Spread: 20.0,
            Osc2Wave: 1.0, Osc2Detune: -7.0, Osc2Mix: 0.3,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 1500.0, FilterResonance: 0.3, FilterEnvAmount: 0.0,
            FilterAttack: 0.005, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.3,
            AmpAttack: 0.05, AmpDecay: 0.3, AmpSustain: 1.0, AmpRelease: 0.5,
            LfoTarget: 1.0, LfoRate: 8.0, LfoDepth: 1.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.6,
            DriveAmount: 0.2, DriveMix: 0.3,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.3, ChorusMix: 0.4, SidechainAmount: 0.5, SidechainRelease: 0.25,
            DelayTime: 0.0, DelayFeedback: 0.5, DelayMix: 0.3, ReverbSize: 0.7, ReverbDamping: 0.4, ReverbMix: 0.3,
        ],
    },
    // Hollow Pad: squares in unison (only odd harmonics: hollow) with a sine, a 1.8 kHz
    // lowpass that takes two seconds to open, 1.5 s / 2.5 s envelope, chorus, big hall.
    Preset {
        name: "Hollow Pad",
        category: Category::EdmPads,
        tags: &["edm", "pad", "hollow", "square", "soft", "ambient", "slow"],
        values: values![
            Osc1Wave: 2.0, Osc1Pulse: 0.5, Osc1Mix: 0.6, Osc1Unison: 3.0, Osc1Spread: 12.0,
            Osc2Wave: 0.0, Osc2Detune: -3.0, Osc2Mix: 0.3,
            SubLevel: 0.1, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 1800.0, FilterResonance: 0.1, FilterEnvAmount: 0.4,
            FilterAttack: 2.0, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 2.0, FilterKeyTrack: 0.3,
            AmpAttack: 1.5, AmpDecay: 1.0, AmpSustain: 0.9, AmpRelease: 2.5,
            LfoTarget: 1.0, LfoRate: 0.2, LfoDepth: 0.1, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.6,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.4, ChorusDepth: 0.4, ChorusMix: 0.5, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.15, ReverbSize: 0.9, ReverbDamping: 0.4, ReverbMix: 0.55,
        ],
    },
    // Future Pad: supersaw + pulse with a filter envelope that swells and settles (0.5 s
    // attack, 1 s decay to 40 %), drive, a 60 % chorus and an 80 % pump: future bass pad.
    Preset {
        name: "Future Pad",
        category: Category::EdmPads,
        tags: &[
            "edm",
            "pad",
            "future bass",
            "sidechain",
            "bright",
            "wide",
            "chord",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.7, Osc1Unison: 7.0, Osc1Spread: 20.0,
            Osc2Wave: 2.0, Osc2Pulse: 0.4, Osc2Detune: 6.0, Osc2Mix: 0.3,
            SubLevel: 0.1, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 2000.0, FilterResonance: 0.15, FilterEnvAmount: 0.5,
            FilterAttack: 0.5, FilterDecay: 1.0, FilterSustain: 0.4, FilterRelease: 1.0, FilterKeyTrack: 0.3,
            AmpAttack: 0.3, AmpDecay: 1.0, AmpSustain: 0.7, AmpRelease: 1.0,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 0.6,
            DriveAmount: 0.3, DriveMix: 0.4,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.6, ChorusDepth: 0.4, ChorusMix: 0.6, SidechainAmount: 0.8, SidechainRelease: 0.35,
            DelayTime: 1.0, DelayFeedback: 0.35, DelayMix: 0.2, ReverbSize: 0.75, ReverbDamping: 0.4, ReverbMix: 0.4,
        ],
    },
    // Sine Bed: two sines a few cents apart over a sub, no filter movement, 1.5 s / 3 s
    // envelope, a half-note echo at low feedback and a huge soft reverb: an ambient bed.
    Preset {
        name: "Sine Bed",
        category: Category::EdmPads,
        tags: &["edm", "pad", "sine", "ambient", "soft", "clean", "bed"],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 0.8, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 5.0, Osc2Mix: 0.6,
            SubLevel: 0.3, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 3000.0, FilterResonance: 0.0, FilterEnvAmount: 0.0,
            FilterAttack: 0.005, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.3,
            AmpAttack: 1.5, AmpDecay: 1.0, AmpSustain: 1.0, AmpRelease: 3.0,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.05,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.0, VelToAmp: 0.5,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.3, ChorusDepth: 0.3, ChorusMix: 0.3, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 3.0, DelayFeedback: 0.3, DelayMix: 0.2, ReverbSize: 0.9, ReverbDamping: 0.3, ReverbMix: 0.6,
        ],
    },
    // Choir Pad: triangles in unison with a little 2:1 FM for a vowel-ish edge through a
    // slowly drifting resonant bandpass at 1.2 kHz, 1 s / 2 s envelope, a 70 % chorus.
    Preset {
        name: "Choir Pad",
        category: Category::EdmPads,
        tags: &["edm", "pad", "choir", "vocal", "soft", "warm", "cinematic"],
        values: values![
            Osc1Wave: 3.0, Osc1Mix: 0.7, Osc1Unison: 5.0, Osc1Spread: 12.0,
            Osc2Wave: 3.0, Osc2Detune: -4.0, Osc2Mix: 0.4,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.1, FmRatio: 2.0, OscMode: 0.0,
            FilterType: 1.0, FilterCutoff: 1200.0, FilterResonance: 0.3, FilterEnvAmount: 0.1,
            FilterAttack: 0.5, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.0, FilterKeyTrack: 0.3,
            AmpAttack: 1.0, AmpDecay: 1.0, AmpSustain: 0.9, AmpRelease: 2.0,
            LfoTarget: 1.0, LfoRate: 0.25, LfoDepth: 0.2, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.6,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.4, ChorusDepth: 0.5, ChorusMix: 0.7, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.15, ReverbSize: 0.85, ReverbDamping: 0.4, ReverbMix: 0.5,
        ],
    },
    // Dark Pad: saw + square an octave-ish down over a heavy sub through a resonant 600 Hz
    // lowpass with a glacial 0.1 Hz sway, drive for grit, 1.2 s / 2.5 s envelope, a 40 %
    // pump and a damped hall.
    Preset {
        name: "Dark Pad",
        category: Category::EdmPads,
        tags: &[
            "edm",
            "pad",
            "dark",
            "cinematic",
            "sub",
            "dubstep",
            "sidechain",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.7, Osc1Unison: 2.0, Osc1Spread: 10.0,
            Osc2Wave: 2.0, Osc2Pulse: 0.5, Osc2Detune: -12.0, Osc2Mix: 0.5,
            SubLevel: 0.5, NoiseLevel: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 600.0, FilterResonance: 0.4, FilterEnvAmount: 0.2,
            FilterAttack: 1.0, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.5, FilterKeyTrack: 0.3,
            AmpAttack: 1.2, AmpDecay: 1.0, AmpSustain: 0.9, AmpRelease: 2.5,
            LfoTarget: 1.0, LfoRate: 0.1, LfoDepth: 0.3, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.5,
            DriveAmount: 0.4, DriveMix: 0.5,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.3, ChorusDepth: 0.3, ChorusMix: 0.3, SidechainAmount: 0.4, SidechainRelease: 0.3,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.1, ReverbSize: 0.85, ReverbDamping: 0.6, ReverbMix: 0.45,
        ],
    },
    // Shimmer Pad: triangles spread 25 cents with 4:1 FM sparkle, a bright 6 kHz lowpass
    // opening over 1.5 s, an 80 % slow chorus, a quarter-note echo and a huge hall.
    Preset {
        name: "Shimmer Pad",
        category: Category::EdmPads,
        tags: &[
            "edm", "pad", "shimmer", "bright", "sparkle", "ambient", "dreamy",
        ],
        values: values![
            Osc1Wave: 3.0, Osc1Mix: 0.7, Osc1Unison: 5.0, Osc1Spread: 25.0,
            Osc2Wave: 0.0, Osc2Detune: 7.0, Osc2Mix: 0.3,
            SubLevel: 0.0, NoiseLevel: 0.0, FmAmount: 0.12, FmRatio: 4.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 6000.0, FilterResonance: 0.05, FilterEnvAmount: 0.3,
            FilterAttack: 1.5, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 2.0, FilterKeyTrack: 0.3,
            AmpAttack: 1.0, AmpDecay: 1.0, AmpSustain: 0.9, AmpRelease: 3.0,
            LfoTarget: 1.0, LfoRate: 0.2, LfoDepth: 0.1, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.6,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.3, ChorusDepth: 0.6, ChorusMix: 0.8, SidechainAmount: 0.3, SidechainRelease: 0.4,
            DelayTime: 2.0, DelayFeedback: 0.5, DelayMix: 0.35, ReverbSize: 0.9, ReverbDamping: 0.3, ReverbMix: 0.6,
        ],
    },
    // ----- Risers & FX ---------------------------------------------------------------------
    // White Riser: white noise only, a 300 Hz lowpass swept four octaves up by a 2 s filter
    // attack while the amp fades in over 2 s; a 1 s release, hall, a 30 % pump.
    Preset {
        name: "White Riser",
        category: Category::Risers,
        tags: &["edm", "riser", "fx", "noise", "white", "build", "sweep"],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.0, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 0.0, NoiseLevel: 1.0, NoiseColor: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 300.0, FilterResonance: 0.3, FilterEnvAmount: 1.0,
            FilterAttack: 2.0, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.0, FilterKeyTrack: 0.0,
            AmpAttack: 2.0, AmpDecay: 0.5, AmpSustain: 1.0, AmpRelease: 1.0,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.0, VelToAmp: 0.5,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.3, SidechainRelease: 0.3,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.1, ReverbSize: 0.8, ReverbDamping: 0.3, ReverbMix: 0.4,
        ],
    },
    // Pink Riser: pink noise through a resonant bandpass sweeping up from 200 Hz over
    // 2 s, a 1.5 s fade-in and 2 s release, chorus for width and a big hall.
    Preset {
        name: "Pink Riser",
        category: Category::Risers,
        tags: &["edm", "riser", "fx", "noise", "pink", "build", "soft"],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.0, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 0.0, NoiseLevel: 1.0, NoiseColor: 1.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 1.0, FilterCutoff: 200.0, FilterResonance: 0.5, FilterEnvAmount: 1.0,
            FilterAttack: 2.0, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.5, FilterKeyTrack: 0.0,
            AmpAttack: 1.5, AmpDecay: 0.5, AmpSustain: 1.0, AmpRelease: 2.0,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.0, VelToAmp: 0.5,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.4, ChorusMix: 0.5, SidechainAmount: 0.2, SidechainRelease: 0.3,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.15, ReverbSize: 0.85, ReverbDamping: 0.3, ReverbMix: 0.5,
        ],
    },
    // Sweep Up: seven spread saws with noise, the pitch envelope rising from two octaves
    // below over 2 s (-24 st decaying to 0) while the lowpass opens four octaves; 1.5 s
    // fade in and out, quarter-note echo, huge hall: the tonal riser.
    Preset {
        name: "Sweep Up",
        category: Category::Risers,
        tags: &["edm", "riser", "fx", "sweep", "pitch", "build", "supersaw"],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.6, Osc1Unison: 7.0, Osc1Spread: 40.0,
            Osc2Wave: 1.0, Osc2Detune: -10.0, Osc2Mix: 0.2,
            SubLevel: 0.0, NoiseLevel: 0.3, NoiseColor: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 800.0, FilterResonance: 0.2, FilterEnvAmount: 1.0,
            FilterAttack: 2.0, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.5, FilterKeyTrack: 0.0,
            AmpAttack: 1.5, AmpDecay: 0.5, AmpSustain: 1.0, AmpRelease: 1.5,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: -24.0, PitchEnvDecay: 2.0, VelToCutoff: 0.0, VelToAmp: 0.5,
            DriveAmount: 0.2, DriveMix: 0.3,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.3, ChorusMix: 0.4, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 2.0, DelayFeedback: 0.45, DelayMix: 0.3, ReverbSize: 0.85, ReverbDamping: 0.3, ReverbMix: 0.5,
        ],
    },
    // Downlifter: spread saws with noise falling two octaves over 2 s (+24 st decaying)
    // while the lowpass closes (negative filter envelope opening over 2 s), a 2 s decay to
    // 30 % and a 2 s release: the post-drop fall.
    Preset {
        name: "Downlifter",
        category: Category::Risers,
        tags: &["edm", "fx", "downlifter", "fall", "sweep", "pitch", "dark"],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.6, Osc1Unison: 5.0, Osc1Spread: 30.0,
            Osc2Wave: 1.0, Osc2Detune: 8.0, Osc2Mix: 0.2,
            SubLevel: 0.0, NoiseLevel: 0.4, NoiseColor: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 6000.0, FilterResonance: 0.2, FilterEnvAmount: -0.8,
            FilterAttack: 2.0, FilterDecay: 1.0, FilterSustain: 1.0, FilterRelease: 1.0, FilterKeyTrack: 0.0,
            AmpAttack: 0.01, AmpDecay: 2.0, AmpSustain: 0.3, AmpRelease: 2.0,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 24.0, PitchEnvDecay: 2.0, VelToCutoff: 0.0, VelToAmp: 0.5,
            DriveAmount: 0.2, DriveMix: 0.3,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.3, ChorusMix: 0.3, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 2.0, DelayFeedback: 0.4, DelayMix: 0.25, ReverbSize: 0.8, ReverbDamping: 0.4, ReverbMix: 0.4,
        ],
    },
    // Impact Sub Drop: sine + heavy sub falling two octaves over 0.6 s into a 400 Hz
    // lowpass, a 2 s decay with no sustain, drive for weight: the impact / sub drop.
    Preset {
        name: "Impact Sub Drop",
        category: Category::Risers,
        tags: &["edm", "fx", "impact", "drop", "sub", "dark", "trap"],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 1.0, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 0.8, NoiseLevel: 0.0, NoiseColor: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 400.0, FilterResonance: 0.1, FilterEnvAmount: 0.3,
            FilterAttack: 0.001, FilterDecay: 0.3, FilterSustain: 0.0, FilterRelease: 0.3, FilterKeyTrack: 0.0,
            AmpAttack: 0.001, AmpDecay: 2.0, AmpSustain: 0.0, AmpRelease: 1.0,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 24.0, PitchEnvDecay: 0.6, VelToCutoff: 0.0, VelToAmp: 0.5,
            DriveAmount: 0.5, DriveMix: 0.5,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.0, ReverbSize: 0.7, ReverbDamping: 0.6, ReverbMix: 0.3,
        ],
    },
    // Dub Echo (drone): a short sine/triangle blip fed into a quarter-note delay at 95 %
    // feedback: the echo regenerates for far longer than 8 s, so it is tagged "drone".
    Preset {
        name: "Dub Echo",
        category: Category::Risers,
        tags: &["fx", "dub", "delay", "echo", "drone", "space", "lofi"],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 0.8, Osc1Unison: 1.0,
            Osc2Wave: 3.0, Osc2Detune: 4.0, Osc2Mix: 0.4,
            SubLevel: 0.0, NoiseLevel: 0.0, NoiseColor: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 2500.0, FilterResonance: 0.2, FilterEnvAmount: 0.4,
            FilterAttack: 0.001, FilterDecay: 0.15, FilterSustain: 0.0, FilterRelease: 0.2, FilterKeyTrack: 0.5,
            AmpAttack: 0.001, AmpDecay: 0.2, AmpSustain: 0.3, AmpRelease: 0.3,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.3, VelToAmp: 0.8,
            DriveAmount: 0.2, DriveMix: 0.3,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.2, LofiNoise: 0.0, LofiTone: 8000.0, LofiMix: 0.5,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 2.0, DelayFeedback: 0.95, DelayMix: 0.6, ReverbSize: 0.6, ReverbDamping: 0.5, ReverbMix: 0.2,
        ],
    },
    // Infinite Hall (drone): a bright sustained saw/square pad at full reverb size (comb
    // feedback 0.98, no damping) and 70 % mix: the hall rings well past 8 s, so "drone".
    Preset {
        name: "Infinite Hall",
        category: Category::Risers,
        tags: &["fx", "pad", "reverb", "drone", "ambient", "space", "bright"],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.7, Osc1Unison: 3.0, Osc1Spread: 15.0,
            Osc2Wave: 2.0, Osc2Pulse: 0.5, Osc2Detune: -6.0, Osc2Mix: 0.4,
            SubLevel: 0.2, NoiseLevel: 0.0, NoiseColor: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 4000.0, FilterResonance: 0.1, FilterEnvAmount: 0.0,
            FilterAttack: 0.005, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.3,
            AmpAttack: 0.2, AmpDecay: 0.5, AmpSustain: 1.0, AmpRelease: 1.0,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.4,
            DriveAmount: 0.3, DriveMix: 0.4,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.4, ChorusDepth: 0.4, ChorusMix: 0.5, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.1, ReverbSize: 1.0, ReverbDamping: 0.0, ReverbMix: 0.7,
        ],
    },
    // Alarm Trill: a narrow pulse trilled a semitone by an 8 Hz pitch LFO at full depth,
    // a resonant 4 kHz lowpass, drive, a sixteenth echo: a siren / alarm FX.
    Preset {
        name: "Alarm Trill",
        category: Category::Risers,
        tags: &[
            "edm",
            "fx",
            "alarm",
            "siren",
            "trill",
            "bright",
            "aggressive",
        ],
        values: values![
            Osc1Wave: 2.0, Osc1Pulse: 0.3, Osc1Mix: 0.9, Osc1Unison: 1.0,
            Osc2Wave: 1.0, Osc2Detune: 0.0, Osc2Mix: 0.2,
            SubLevel: 0.0, NoiseLevel: 0.0, NoiseColor: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 4000.0, FilterResonance: 0.4, FilterEnvAmount: 0.2,
            FilterAttack: 0.005, FilterDecay: 0.25, FilterSustain: 0.5, FilterRelease: 0.3, FilterKeyTrack: 0.3,
            AmpAttack: 0.005, AmpDecay: 0.2, AmpSustain: 1.0, AmpRelease: 0.2,
            LfoTarget: 0.0, LfoRate: 8.0, LfoDepth: 1.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.2, VelToAmp: 0.5,
            DriveAmount: 0.5, DriveMix: 0.6,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 0.0, DelayFeedback: 0.5, DelayMix: 0.3, ReverbSize: 0.6, ReverbDamping: 0.5, ReverbMix: 0.2,
        ],
    },
    // Wind: pink noise through a resonant 800 Hz bandpass drifting +-1.6 octaves at
    // 0.15 Hz, a 1 s fade-in and 2 s release, a big hall: wind / breath texture.
    Preset {
        name: "Wind",
        category: Category::Risers,
        tags: &[
            "fx",
            "wind",
            "noise",
            "texture",
            "ambient",
            "soft",
            "cinematic",
        ],
        values: values![
            Osc1Wave: 1.0, Osc1Mix: 0.0, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 0.0, NoiseLevel: 1.0, NoiseColor: 1.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 1.0, FilterCutoff: 800.0, FilterResonance: 0.6, FilterEnvAmount: 0.0,
            FilterAttack: 0.005, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.0,
            AmpAttack: 1.0, AmpDecay: 0.5, AmpSustain: 1.0, AmpRelease: 2.0,
            LfoTarget: 1.0, LfoRate: 0.15, LfoDepth: 0.8, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.0, VelToAmp: 0.5,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.3, ChorusDepth: 0.4, ChorusMix: 0.4, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 2.0, DelayFeedback: 0.3, DelayMix: 0.1, ReverbSize: 0.85, ReverbDamping: 0.4, ReverbMix: 0.4,
        ],
    },
    // Vinyl Crackle (drone): a faint 200 Hz-lowpassed sine hum under the lo-fi insert's
    // vinyl bed at full level; the bed never stops, hence "drone": a texture layer.
    Preset {
        name: "Vinyl Crackle",
        category: Category::Risers,
        tags: &[
            "lofi", "fx", "vinyl", "crackle", "texture", "drone", "noise",
        ],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 0.3, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 0.2, NoiseLevel: 0.0, NoiseColor: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 200.0, FilterResonance: 0.0, FilterEnvAmount: 0.0,
            FilterAttack: 0.005, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.0,
            AmpAttack: 0.5, AmpDecay: 0.5, AmpSustain: 1.0, AmpRelease: 1.0,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 0.0, PitchEnvDecay: 0.1, VelToCutoff: 0.0, VelToAmp: 0.5,
            DriveAmount: 0.0, DriveMix: 0.0,
            LofiBits: 12.0, LofiRate: 1.0, LofiWobble: 0.3, LofiNoise: 1.0, LofiTone: 8000.0, LofiMix: 1.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.0, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.1,
        ],
    },
    // Laser Zap: a sine falling two octaves in 150 ms, 150 ms decay, no sustain, a
    // sixteenth echo: the zap / laser one-shot.
    Preset {
        name: "Laser Zap",
        category: Category::Risers,
        tags: &["edm", "fx", "laser", "zap", "pitch", "short", "bright"],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 1.0, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 0.0, NoiseLevel: 0.0, NoiseColor: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 12000.0, FilterResonance: 0.0, FilterEnvAmount: 0.0,
            FilterAttack: 0.005, FilterDecay: 0.25, FilterSustain: 0.2, FilterRelease: 0.3, FilterKeyTrack: 0.0,
            AmpAttack: 0.001, AmpDecay: 0.15, AmpSustain: 0.0, AmpRelease: 0.1,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 24.0, PitchEnvDecay: 0.15, VelToCutoff: 0.0, VelToAmp: 0.7,
            DriveAmount: 0.1, DriveMix: 0.2,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 0.0, DelayFeedback: 0.5, DelayMix: 0.3, ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.2,
        ],
    },
    // Thunder Boom: pink noise over a full sub, a 300 Hz lowpass kicked open and closing
    // over a second, a 12-semitone drop, 2 s decay, drive and a damped huge hall.
    Preset {
        name: "Thunder Boom",
        category: Category::Risers,
        tags: &[
            "edm",
            "fx",
            "impact",
            "boom",
            "thunder",
            "dark",
            "cinematic",
        ],
        values: values![
            Osc1Wave: 0.0, Osc1Mix: 0.6, Osc1Unison: 1.0,
            Osc2Wave: 0.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
            SubLevel: 1.0, NoiseLevel: 0.8, NoiseColor: 1.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
            FilterType: 0.0, FilterCutoff: 300.0, FilterResonance: 0.2, FilterEnvAmount: 0.8,
            FilterAttack: 0.001, FilterDecay: 1.0, FilterSustain: 0.0, FilterRelease: 0.5, FilterKeyTrack: 0.0,
            AmpAttack: 0.001, AmpDecay: 2.0, AmpSustain: 0.0, AmpRelease: 1.5,
            LfoTarget: 1.0, LfoRate: 0.3, LfoDepth: 0.0, Glide: 0.0,
            PitchEnvAmount: 12.0, PitchEnvDecay: 0.3, VelToCutoff: 0.0, VelToAmp: 0.5,
            DriveAmount: 0.4, DriveMix: 0.5,
            LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0, LofiMix: 0.0,
            ChorusRate: 0.5, ChorusDepth: 0.2, ChorusMix: 0.0, SidechainAmount: 0.0, SidechainRelease: 0.25,
            DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.0, ReverbSize: 0.9, ReverbDamping: 0.7, ReverbMix: 0.5,
        ],
    },
];

/// Number of presets in [`LIBRARY`].
pub const LIBRARY_COUNT: usize = LIBRARY.len();

/// The library presets of one category, in library order.
pub fn by_category(category: Category) -> impl Iterator<Item = &'static Preset> {
    LIBRARY.iter().filter(move |p| p.category == category)
}

/// Finds a preset by name (ASCII case-insensitive) in [`LIBRARY`], then in [`ALL`].
#[must_use]
pub fn find(name: &str) -> Option<&'static Preset> {
    LIBRARY
        .iter()
        .chain(ALL.iter())
        .find(|p| p.name.eq_ignore_ascii_case(name))
}

/// Index of the preset named `name` (ASCII case-insensitive) in [`LIBRARY`]; `None` for
/// unknown names and for the original eight (they are not in the library).
#[must_use]
pub fn index_of(name: &str) -> Option<usize> {
    LIBRARY
        .iter()
        .position(|p| p.name.eq_ignore_ascii_case(name))
}

/// Library presets whose name or one of whose tags contains `query` (ASCII
/// case-insensitive substring; surrounding whitespace ignored), in library order. An empty
/// query returns the whole library.
#[must_use]
pub fn search(query: &str) -> Vec<&'static Preset> {
    let q = query.trim().to_ascii_lowercase();
    LIBRARY
        .iter()
        .filter(|p| {
            q.is_empty()
                || p.name.to_ascii_lowercase().contains(&q)
                || p.tags.iter().any(|t| t.to_ascii_lowercase().contains(&q))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_findable() {
        assert_eq!(ALL.len(), 8);
        for (i, p) in ALL.iter().enumerate() {
            assert!(ALL[..i].iter().all(|q| q.name != p.name), "{}", p.name);
            assert_eq!(find(p.name).map(|q| q.name), Some(p.name));
            assert_eq!(
                find(&p.name.to_ascii_uppercase()).map(|q| q.name),
                Some(p.name)
            );
            assert!(index_of(p.name).is_none(), "{} is not in LIBRARY", p.name);
        }
        assert!(find("Nope").is_none());
    }

    #[test]
    fn values_are_in_range_and_not_duplicated() {
        for p in ALL.iter().chain(LIBRARY) {
            for (i, (id, v)) in p.values.iter().enumerate() {
                assert!(id.range().contains(v), "{} {id:?} = {v}", p.name);
                assert!(
                    p.values[..i].iter().all(|(other, _)| other != id),
                    "{} lists {id:?} twice",
                    p.name
                );
            }
            assert_eq!(p.value(ParamId::MasterGain), 1.0);
        }
        assert_eq!(ACID.value(ParamId::FilterResonance), 0.85);
    }

    #[test]
    fn presets_sound_different() {
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                let differing = ParamId::ALL
                    .iter()
                    .filter(|id| id.is_synth() && a.value(**id) != b.value(**id))
                    .count();
                assert!(
                    differing >= 4,
                    "{} vs {} differ in {differing}",
                    a.name,
                    b.name
                );
            }
        }
    }

    #[test]
    fn categories_round_trip_and_have_labels() {
        for (i, c) in Category::ALL.iter().enumerate() {
            assert_eq!(c.index(), i);
            assert_eq!(Category::from_index(i), *c);
            assert!(!c.label().is_empty());
        }
        assert_eq!(
            Category::from_index(Category::ALL.len()),
            Category::LofiKeys
        );
        assert_eq!(Category::Risers.label(), "Risers & FX");
    }

    #[test]
    fn library_is_grouped_by_category_and_tags_helpers_work() {
        assert_eq!(LIBRARY_COUNT, LIBRARY.len());
        let mut last = 0;
        for p in LIBRARY {
            assert!(
                p.category.index() >= last,
                "{} out of category order",
                p.name
            );
            last = p.category.index();
        }
        let dusty = find("dusty rhodes").expect("Dusty Rhodes");
        assert!(dusty.has_tag("LOFI"));
        assert!(!dusty.is_drone());
        assert!(dusty.has_vinyl_noise());
        assert!(dusty.lists(ParamId::LofiBits));
        assert!(find("Dub Echo").expect("Dub Echo").is_drone());
        assert!(search("   ").len() == LIBRARY_COUNT);
    }
}
