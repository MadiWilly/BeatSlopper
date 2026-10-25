//! Instrument tracks (M10): up to [`MAX_TRACKS`] synth layers, each with its own voice pool,
//! its own per-track parameters and its own stereo insert chain.
//!
//! Before M10 the engine had one synth: one [`VoicePool`], one set of synth parameters, one
//! insert (drive -> lo-fi -> chorus -> sidechain ducker, formerly inside `FxChain`), one note
//! list per pattern and one `Synth` mixer strip. A [`Track`] bundles exactly those per-synth
//! pieces, so the engine can hold eight of them and a user can layer a pad roll under a keys
//! roll. What stays global: delay, reverb, EQ, master gain, limiter, BPM, swing and the drum
//! kit.
//!
//! - [`Track`] is the audio-side struct: fixed-size, allocation-free after construction,
//!   `Copy`-free but memswap-able (so `SwapTracks` is a plain swap). `params` holds every
//!   [`ParamId`] slot but only the per-track ids ([`ParamId::is_track`]) are meaningful.
//! - [`SynthInsert`] is the per-track stereo insert: `drive -> lo-fi -> chorus -> x ducker`.
//!   The drive's amount and mix are smoothed here (they used to tick in `FxChain`); the
//!   smoothers advance once per [`SynthInsert::process`] call, *after* the drive has used
//!   them, which keeps the pre-M10 sample sequence bit for bit.
//! - [`TrackState`] is the serde form (project file, UI mirror): name (at most
//!   [`MAX_NAME_LEN`] chars), colour index (`0..COLOUR_COUNT`), active flag, preset name and
//!   the per-track parameters keyed by [`ParamId::key`]. Names, colours and preset names are
//!   UI-side data; the engine never sees them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::fx::{Chorus, Drive, Ducker, Lofi};
use crate::params::{DEFAULT_SMOOTH_MS, ParamId, Smoother};
use crate::voice::{SynthParams, VoicePool};

/// Number of instrument track slots.
pub const MAX_TRACKS: usize = 8;

/// Index of a track slot, `0..MAX_TRACKS`.
pub type TrackId = u8;

/// Longest track name, in characters.
pub const MAX_NAME_LEN: usize = 16;

/// Number of track colour swatches (`TrackState::colour` is `0..COLOUR_COUNT`).
pub const COLOUR_COUNT: u8 = 8;

/// Wraps a track index into `0..MAX_TRACKS`.
#[inline]
#[must_use]
pub const fn track_index(track: TrackId) -> usize {
    (track as usize) % MAX_TRACKS
}

/// Name a fresh track in `slot` gets: `"Synth"` for slot 0 (the pre-M10 synth), `"Track N"`
/// otherwise.
#[must_use]
pub fn default_name(slot: TrackId) -> String {
    if track_index(slot) == 0 {
        "Synth".to_owned()
    } else {
        format!("Track {}", track_index(slot) + 1)
    }
}

// --- SynthInsert --------------------------------------------------------------------------

/// The per-track stereo insert: `drive -> lo-fi -> chorus -> x ducker gain`, mono in, stereo
/// out. Owns the smoothers of the drive amount and mix; the lo-fi and chorus stages smooth
/// their own parameters.
///
/// Real-time safe after [`SynthInsert::new`]: nothing here allocates, locks, logs or panics.
#[derive(Debug, Clone, PartialEq)]
pub struct SynthInsert {
    sample_rate: f32,
    drive: Drive,
    lofi: Lofi,
    chorus: Chorus,
    ducker: Ducker,
    drive_amount: Smoother,
    drive_mix: Smoother,
    sidechain_amount: f32,
    sidechain_release: f32,
}

impl SynthInsert {
    /// Pre-allocates the insert for `sample_rate` (non-finite or non-positive rates fall back
    /// to 48 kHz) with every parameter at its [`ParamId::default_value`].
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let smoother =
            |id: ParamId| Smoother::new(sample_rate, DEFAULT_SMOOTH_MS, id.default_value());
        let mut insert = Self {
            sample_rate,
            drive: Drive::new(),
            lofi: Lofi::new(sample_rate),
            chorus: Chorus::new(sample_rate),
            ducker: Ducker::new(sample_rate),
            drive_amount: smoother(ParamId::DriveAmount),
            drive_mix: smoother(ParamId::DriveMix),
            sidechain_amount: ParamId::SidechainAmount.default_value(),
            sidechain_release: ParamId::SidechainRelease.default_value(),
        };
        insert
            .ducker
            .set(insert.sidechain_amount, insert.sidechain_release);
        insert.reset();
        insert
    }

    /// Sample rate the insert was built for.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Sets one insert parameter (clamped with [`ParamId::clamp`]). Returns `false` and
    /// changes nothing for ids that are not [`ParamId::is_synth_insert`]. The drive amount and
    /// mix glide over about 10 ms; lo-fi and chorus smooth internally; the sidechain values
    /// reach the ducker at once.
    pub fn set_param(&mut self, id: ParamId, value: f32) -> bool {
        if !id.is_synth_insert() {
            return false;
        }
        let v = id.clamp(value);
        match id {
            ParamId::DriveAmount => self.drive_amount.set_target(v),
            ParamId::DriveMix => self.drive_mix.set_target(v),
            ParamId::LofiBits
            | ParamId::LofiRate
            | ParamId::LofiWobble
            | ParamId::LofiNoise
            | ParamId::LofiTone
            | ParamId::LofiMix => self.lofi.set_param(id, v),
            ParamId::ChorusRate | ParamId::ChorusDepth | ParamId::ChorusMix => {
                self.chorus.set_param(id, v);
            }
            ParamId::SidechainAmount => {
                self.sidechain_amount = v;
                self.ducker.set(v, self.sidechain_release);
            }
            ParamId::SidechainRelease => {
                self.sidechain_release = v;
                self.ducker.set(self.sidechain_amount, v);
            }
            _ => return false,
        }
        true
    }

    /// Current (clamped target) value of an insert parameter; other ids return `0.0`.
    #[must_use]
    pub fn param(&self, id: ParamId) -> f32 {
        match id {
            ParamId::DriveAmount => self.drive_amount.target(),
            ParamId::DriveMix => self.drive_mix.target(),
            ParamId::LofiBits
            | ParamId::LofiRate
            | ParamId::LofiWobble
            | ParamId::LofiNoise
            | ParamId::LofiTone
            | ParamId::LofiMix => self.lofi.param(id),
            ParamId::ChorusRate | ParamId::ChorusDepth | ParamId::ChorusMix => {
                self.chorus.param(id)
            }
            ParamId::SidechainAmount => self.sidechain_amount,
            ParamId::SidechainRelease => self.sidechain_release,
            _ => 0.0,
        }
    }

    /// Starts a sidechain dip (the engine calls this on every kick hit).
    pub fn trigger_ducker(&mut self) {
        self.ducker.trigger();
    }

    /// The drive stage with its current (smoothed) amount and mix.
    #[must_use]
    pub fn drive(&self) -> &Drive {
        &self.drive
    }

    /// The lo-fi stage.
    #[must_use]
    pub fn lofi(&self) -> &Lofi {
        &self.lofi
    }

    /// The chorus stage.
    #[must_use]
    pub fn chorus(&self) -> &Chorus {
        &self.chorus
    }

    /// The sidechain ducker.
    #[must_use]
    pub fn ducker(&self) -> &Ducker {
        &self.ducker
    }

    /// Clears the lo-fi and chorus lines, resets the ducker and snaps the drive smoothers to
    /// their targets. May click; use it on project load or while silent.
    pub fn reset(&mut self) {
        self.lofi.reset();
        self.chorus.reset();
        self.ducker.reset();
        self.snap();
    }

    /// Snaps the drive smoothers to their targets, leaving every line and tail alone.
    pub fn snap(&mut self) {
        self.drive_amount.snap(self.drive_amount.target());
        self.drive_mix.snap(self.drive_mix.target());
        self.apply_smoothed();
    }

    #[inline]
    fn apply_smoothed(&mut self) {
        self.drive.set_amount(self.drive_amount.current());
        self.drive.set_mix(self.drive_mix.current());
    }

    /// One sample: `drive -> lo-fi -> chorus -> x ducker gain`. The drive uses the smoothed
    /// amount/mix as of the previous call (the smoothers advance after it, once per call), the
    /// lo-fi and chorus tick their own smoothers and the ducker advances one sample.
    ///
    /// With `LofiMix = 0`, `ChorusMix = 0` and no dip running, the result is the drive output
    /// on both channels exactly. Non-finite input is treated as `0.0`; the output is finite.
    #[inline]
    pub fn process(&mut self, x: f32) -> (f32, f32) {
        let x = self.drive.process(x);
        let x = self.lofi.process(x);
        let (l, r) = self.chorus.process(x);
        let g = self.ducker.tick();
        self.drive_amount.tick();
        self.drive_mix.tick();
        self.apply_smoothed();
        (l * g, r * g)
    }
}

// --- Track --------------------------------------------------------------------------------

/// One instrument track on the audio side: its activity flag, voice pool, insert and the
/// current value of every per-track parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    /// Whether the track exists for the user. An inactive track is skipped by the render loop
    /// (once its voices have faded) and has no notes in any pattern.
    pub active: bool,
    /// The track's voices (16, with stealing).
    pub pool: VoicePool,
    /// The track's stereo insert.
    pub insert: SynthInsert,
    /// Current (clamped) value of every [`ParamId`], indexed by [`ParamId::index`]; only the
    /// [`ParamId::is_track`] ids are meaningful, the rest stay at their defaults.
    pub params: [f32; ParamId::COUNT],
}

impl Track {
    /// An inactive track with every parameter at its default.
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let mut params = [0.0_f32; ParamId::COUNT];
        for id in ParamId::ALL {
            params[id.index()] = id.default_value();
        }
        Self {
            active: false,
            pool: VoicePool::new(sample_rate),
            insert: SynthInsert::new(sample_rate),
            params,
        }
    }

    /// Sets a per-track parameter (clamped) and applies it to the voices or the insert.
    /// Returns the stored value; ids that are not [`ParamId::is_track`] change nothing and
    /// return their clamped input.
    pub fn set_param(&mut self, id: ParamId, value: f32) -> f32 {
        let value = id.clamp(value);
        if !id.is_track() {
            return value;
        }
        self.params[id.index()] = value;
        if id.is_synth() {
            self.pool.set_param(id, value);
        } else {
            self.insert.set_param(id, value);
        }
        value
    }

    /// Current (clamped) value of parameter `id`.
    #[must_use]
    pub fn param(&self, id: ParamId) -> f32 {
        self.params[id.index()]
    }

    /// The synth parameters currently applied to the voices.
    #[must_use]
    pub fn synth_params(&self) -> &SynthParams {
        self.pool.params()
    }

    /// Every per-track parameter back to its default (voices and insert follow).
    pub fn reset_params(&mut self) {
        for &id in ParamId::ALL {
            if id.is_track() {
                self.set_param(id, id.default_value());
            }
        }
    }

    /// Copies every per-track parameter from `other` (voices and insert follow); notes,
    /// voices and the mixer strip are not part of this.
    pub fn copy_settings_from(&mut self, other: &Track) {
        for &id in ParamId::ALL {
            if id.is_track() {
                self.set_param(id, other.params[id.index()]);
            }
        }
    }

    /// `true` while at least one voice is sounding (held or releasing).
    #[must_use]
    pub fn is_sounding(&self) -> bool {
        self.pool.is_active()
    }
}

// --- TrackState ---------------------------------------------------------------------------

/// Serde / UI form of one track: slot, name, colour, activity, preset name and the per-track
/// parameters keyed by [`ParamId::key`] (only [`ParamId::is_track`] ids).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackState {
    /// Track slot `0..MAX_TRACKS` this state describes.
    pub slot: u8,
    /// Display name, at most [`MAX_NAME_LEN`] characters.
    pub name: String,
    /// Colour swatch index, `0..COLOUR_COUNT`.
    pub colour: u8,
    /// Whether the track exists for the user.
    pub active: bool,
    /// Name of the preset the track was built from (`""` when unknown or modified).
    pub preset: String,
    /// Per-track parameters keyed by [`ParamId::key`]; missing ids take their defaults.
    pub params: BTreeMap<String, f32>,
}

impl Default for TrackState {
    /// An active slot-0 track named `"Synth"`, colour 0, no preset, default parameters.
    fn default() -> Self {
        Self::new(0)
    }
}

impl TrackState {
    /// An active track in `slot` with its [`default_name`], colour `slot % COLOUR_COUNT`, no
    /// preset name and every per-track parameter at its default.
    #[must_use]
    pub fn new(slot: TrackId) -> Self {
        let slot = track_index(slot) as u8;
        Self {
            slot,
            name: default_name(slot),
            colour: slot % COLOUR_COUNT,
            active: true,
            preset: String::new(),
            params: default_track_params(),
        }
    }

    /// Snapshot of an audio-side [`Track`] in `slot` (name, colour and preset are UI data and
    /// are set to the defaults of [`TrackState::new`]).
    #[must_use]
    pub fn from_track(slot: TrackId, track: &Track) -> Self {
        let mut state = Self::new(slot);
        state.active = track.active;
        for &id in ParamId::ALL {
            if id.is_track() {
                state
                    .params
                    .insert(id.key().to_owned(), track.params[id.index()]);
            }
        }
        state
    }

    /// Value of parameter `id`, or its default when the state does not list it.
    #[must_use]
    pub fn param(&self, id: ParamId) -> f32 {
        self.params
            .get(id.key())
            .copied()
            .unwrap_or_else(|| id.default_value())
    }

    /// Sets parameter `id` (per-track ids only; others are ignored).
    pub fn set_param(&mut self, id: ParamId, value: f32) {
        if id.is_track() {
            self.params.insert(id.key().to_owned(), value);
        }
    }

    /// Pushes every per-track parameter into `track` (clamped) and copies the active flag.
    pub fn apply_to(&self, track: &mut Track) {
        track.active = self.active;
        for &id in ParamId::ALL {
            if id.is_track() {
                track.set_param(id, self.param(id));
            }
        }
    }

    /// Clamps the slot and colour, truncates the name to [`MAX_NAME_LEN`] characters, drops
    /// unknown and non-track parameter keys and fills missing ones with their defaults.
    pub fn normalise(&mut self) {
        self.slot = track_index(self.slot) as u8;
        self.colour %= COLOUR_COUNT;
        if let Some((idx, _)) = self.name.char_indices().nth(MAX_NAME_LEN) {
            self.name.truncate(idx);
        }
        self.params
            .retain(|key, _| ParamId::from_key(key).is_some_and(ParamId::is_track));
        for &id in ParamId::ALL {
            if id.is_track() {
                let v = self
                    .params
                    .entry(id.key().to_owned())
                    .or_insert_with(|| id.default_value());
                *v = id.clamp(*v);
            }
        }
    }
}

/// Every per-track parameter at its default, keyed by [`ParamId::key`].
#[must_use]
pub fn default_track_params() -> BTreeMap<String, f32> {
    ParamId::ALL
        .iter()
        .filter(|id| id.is_track())
        .map(|id| (id.key().to_owned(), id.default_value()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn param_classes_partition_the_ids() {
        for &id in ParamId::ALL {
            assert_ne!(id.is_track(), id.is_global(), "{id:?}");
            assert!(!(id.is_synth() && id.is_synth_insert()), "{id:?}");
        }
        assert!(ParamId::DriveMix.is_synth_insert());
        assert!(ParamId::SidechainRelease.is_track());
        assert!(ParamId::DelayMix.is_global());
        assert!(ParamId::MasterGain.is_global());
        assert!(ParamId::FilterCutoff.is_track());
    }

    #[test]
    fn insert_params_round_trip_and_filter() {
        let mut ins = SynthInsert::new(SR);
        assert_eq!(ins.sample_rate(), SR);
        for &id in ParamId::ALL {
            if id.is_synth_insert() {
                assert_eq!(ins.param(id), id.default_value(), "{id:?}");
                let hi = *id.range().end();
                assert!(ins.set_param(id, hi + 1.0));
                assert_eq!(ins.param(id), hi, "{id:?}");
            } else {
                assert!(!ins.set_param(id, 1.0));
                assert_eq!(ins.param(id), 0.0);
            }
        }
        assert_eq!(ins.ducker().amount(), 1.0);
        assert_eq!(ins.ducker().release(), 1.0);
        assert_eq!(SynthInsert::new(0.0).sample_rate(), 48_000.0);
    }

    #[test]
    fn insert_is_transparent_at_the_dry_settings() {
        let mut ins = SynthInsert::new(SR);
        ins.set_param(ParamId::DriveMix, 0.0);
        ins.set_param(ParamId::DriveAmount, 0.0);
        ins.reset();
        for i in 0..4_800 {
            let x = (i as f32 * 0.05).sin() * 0.4;
            assert_eq!(ins.process(x), (x, x), "sample {i}");
        }
        let (l, r) = ins.process(f32::NAN);
        assert!(l.is_finite() && r.is_finite());
    }

    #[test]
    fn insert_drive_smoothers_glide_after_the_sample_that_used_them() {
        // The drive uses the value as of the previous call, so the very first sample after a
        // change still sees the old amount (the pre-M10 ordering).
        let mut ins = SynthInsert::new(SR);
        ins.set_param(ParamId::DriveMix, 0.0);
        ins.reset();
        assert_eq!(ins.process(0.3), (0.3, 0.3));
        ins.set_param(ParamId::DriveMix, 1.0);
        ins.set_param(ParamId::DriveAmount, 1.0);
        assert_eq!(ins.process(0.3), (0.3, 0.3), "old mix on this sample");
        let (l, _) = ins.process(0.3);
        assert_ne!(l, 0.3, "the glide has started");
        let mut last = l;
        for _ in 0..4_800 {
            last = ins.process(0.3).0;
        }
        assert!(ins.drive().mix() > 0.999 && ins.drive().amount() > 0.999);
        assert!(last > 0.3, "fully driven 0.3 is louder: {last}");
    }

    #[test]
    fn track_routes_params_to_pool_and_insert() {
        let mut t = Track::new(SR);
        assert!(!t.active);
        assert_eq!(t.set_param(ParamId::FilterCutoff, 500.0), 500.0);
        assert_eq!(t.synth_params().cutoff, 500.0);
        assert_eq!(t.set_param(ParamId::LofiMix, 2.0), 1.0);
        assert_eq!(t.insert.param(ParamId::LofiMix), 1.0);
        assert_eq!(t.param(ParamId::LofiMix), 1.0);
        // Global ids are not stored on a track.
        assert_eq!(t.set_param(ParamId::MasterGain, 0.1), 0.1);
        assert_eq!(
            t.param(ParamId::MasterGain),
            ParamId::MasterGain.default_value()
        );
        let mut u = Track::new(SR);
        u.copy_settings_from(&t);
        assert_eq!(u.params, t.params);
        assert_eq!(u.synth_params().cutoff, 500.0);
        t.reset_params();
        assert_eq!(
            t.param(ParamId::FilterCutoff),
            ParamId::FilterCutoff.default_value()
        );
        assert_eq!(t.insert.param(ParamId::LofiMix), 0.0);
        assert!(!t.is_sounding());
        t.pool.note_on(60, 1.0);
        assert!(t.is_sounding());
    }

    #[test]
    fn track_state_defaults_normalise_and_round_trip() {
        let s = TrackState::default();
        assert_eq!(s.slot, 0);
        assert_eq!(s.name, "Synth");
        assert!(s.active);
        assert_eq!(
            s.params.len(),
            ParamId::ALL.iter().filter(|id| id.is_track()).count()
        );
        assert_eq!(TrackState::new(3).name, "Track 4");
        assert_eq!(TrackState::new(9).slot, 1, "slots wrap");
        let mut odd = TrackState {
            slot: 200,
            name: "a very long track name indeed".to_owned(),
            colour: 17,
            ..TrackState::new(1)
        };
        odd.params.insert("flux".to_owned(), 1.0);
        odd.params.insert(ParamId::MasterGain.key().to_owned(), 1.0);
        odd.params
            .insert(ParamId::FilterCutoff.key().to_owned(), 1.0e9);
        odd.params.remove(ParamId::LofiMix.key());
        odd.normalise();
        assert_eq!(odd.slot, 0);
        assert_eq!(odd.colour, 1);
        assert_eq!(odd.name.chars().count(), MAX_NAME_LEN);
        assert!(!odd.params.contains_key("flux"));
        assert!(!odd.params.contains_key(ParamId::MasterGain.key()));
        assert_eq!(
            odd.param(ParamId::FilterCutoff),
            *ParamId::FilterCutoff.range().end()
        );
        assert_eq!(odd.param(ParamId::LofiMix), 0.0);
        let json = serde_json::to_string(&odd).unwrap();
        let back: TrackState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, odd);
        let sparse: TrackState = serde_json::from_str(r#"{ "slot": 2, "name": "Keys" }"#).unwrap();
        assert_eq!(sparse.name, "Keys");
        assert!(sparse.active);
        assert_eq!(
            sparse.param(ParamId::Osc1Mix),
            ParamId::Osc1Mix.default_value()
        );

        // apply_to / from_track agree.
        let mut t = Track::new(SR);
        odd.apply_to(&mut t);
        assert!(t.active);
        assert_eq!(
            t.synth_params().cutoff,
            *ParamId::FilterCutoff.range().end()
        );
        let snap = TrackState::from_track(0, &t);
        assert_eq!(snap.params, odd.params);
    }
}
