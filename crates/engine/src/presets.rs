//! Built-in sound presets: plain `(ParamId, value)` data so the UI can apply them through
//! `Message::SetParam` as well.
//!
//! [`Preset::apply`] first resets **every** parameter to its default (master gain and limiter
//! included), then applies the preset's values; a preset therefore fully describes a sound
//! and parameters it does not list are at their defaults.

use crate::engine::Engine;
use crate::params::ParamId;

/// A named set of parameter values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preset {
    /// Display name (unique within [`ALL`]).
    pub name: &'static str,
    /// Parameter values; ids not listed are at their [`ParamId::default_value`].
    pub values: &'static [(ParamId, f32)],
}

impl Preset {
    /// Resets every parameter to its default, then applies this preset's values.
    pub fn apply(&self, engine: &mut Engine) {
        for &id in ParamId::ALL {
            engine.set_param(id, id.default_value());
        }
        for &(id, value) in self.values {
            engine.set_param(id, value);
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
}

use ParamId as P;

/// Punchy mono-style bass: saw + square an octave-ish apart, closed filter, drive.
pub const BASS: Preset = Preset {
    name: "Bass",
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

/// Every built-in preset, in menu order.
pub const ALL: &[Preset] = &[BASS, SUB, LEAD, PLUCK, PAD, KEYS, ACID, BELL];

/// Finds a preset by name (ASCII case-insensitive).
#[must_use]
pub fn find(name: &str) -> Option<&'static Preset> {
    ALL.iter().find(|p| p.name.eq_ignore_ascii_case(name))
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
        }
        assert!(find("Nope").is_none());
    }

    #[test]
    fn values_are_in_range_and_not_duplicated() {
        for p in ALL {
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
}
