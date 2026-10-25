//! Project file (M3/M5/M6/M7/M10): `*.beatslopper.json` holding BPM, swing, the pattern bank
//! (eight patterns with their per-track notes, chain, play mode), the global FX / master
//! params, the mixer, the master EQ and reverb, the instrument tracks and the keyboard
//! settings.
//!
//! Serialization happens on the UI thread only; the audio thread receives the loaded state
//! through [`Project::apply_to`] (before the stream starts) or as small `Copy` messages.
//!
//! Layout ([`FORMAT_VERSION`] 8), human-readable and stable:
//!
//! ```json
//! {
//!   "version": 8,
//!   "bpm": 120.0,
//!   "swing": 0.0,
//!   "bank": {
//!     "patterns": [
//!       { "length": 16, "drums": [[...], [...], [...], [...], [...]],
//!         "tracks": [ { "track": 0, "notes": [ { "start": 0, "length": 1, "pitch": 36, "velocity": 0.8 } ] } ],
//!         "drum_volume": [...], "drum_tune": [...] },
//!       "... 8 patterns in all (A..H)"
//!     ],
//!     "chain": [ { "pattern": 0, "repeats": 1 } ],
//!     "mode": "pattern",
//!     "current": 0
//!   },
//!   "params": { "delay_feedback": 0.35, "master_gain": 0.8, "...": 0.0 },
//!   "mixer": {
//!     "strips": [
//!       { "level_db": 0.0, "pan": 0.0, "mute": false, "solo": false,
//!         "delay_send": 0.0, "reverb_send": 0.0 },
//!       "... 13 strips in all: kick, snare, clap, closed hat, open hat, tracks 1..8"
//!     ],
//!     "inserts": [
//!       { "level_db": 0.0, "pan": 0.0, "mute": false, "solo": false,
//!         "delay_send": 0.0, "reverb_send": 0.0,
//!         "eq3": { "low_gain_db": 0.0, "mid_gain_db": 0.0, "mid_freq_hz": 1000.0, "high_gain_db": 0.0 },
//!         "slots": [ { "kind": "off", "params": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0] }, "... 3 slots" ],
//!         "route": 0 },
//!       "... 16 inserts in all (M11)"
//!     ],
//!     "routes": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]
//!   },
//!   "eq": { "...": "the master EQ" },
//!   "reverb": { "...": "every reverb value" },
//!   "tracks": [
//!     { "slot": 0, "name": "Synth", "colour": 0, "active": true, "preset": "Bass",
//!       "params": { "amp_attack": 0.01, "osc1_wave": 1.0, "drive_mix": 0.5, "...": 0.0 } }
//!   ],
//!   "octave": 4,
//!   "velocity": 0.8
//! }
//! ```
//!
//! Each drum row is a plain array of 16, 32 or 64 step objects and a pattern's `tracks` list
//! the notes of every non-empty track (see [`crate::pattern`] for the exact rules; the derived
//! `synth` lane is not written). `params` holds only the **global** parameters
//! ([`ParamId::is_global`]: delay, reverb, master gain, limiter); every per-track parameter
//! ([`ParamId::is_track`]) lives inside its [`TrackState`]. Only active tracks need to be
//! written; each entry names its `slot`. Parameters are keyed by [`ParamId::key`] so renaming
//! a UI label never breaks a file. On load, unknown parameter keys are ignored and missing
//! ones take [`ParamId::default_value`]; missing top-level fields take their
//! [`Project::default`] value (a missing `tracks` field means one default `"Synth"` track; an
//! explicit empty array means no track).
//!
//! Migration: a version-7 (and older) file keeps its mix on the source strips and has no
//! `inserts` / `routes`: [`MixerState::migrate_pre_m11`] moves every written strip onto the
//! insert of the same number (level, pan, mute, solo, sends), resets the strip to neutral
//! and routes the source there, so the file sounds as before (channels the file did not
//! write get the insert defaults). A version-6 (and older) file has one synth: its `params`
//! hold the track ids too, each pattern has a single `notes` list and the mixer has six
//! strips. It loads as one
//! track in slot 0 named `"Synth"`, colour 0, holding the file's synth and insert params, with
//! the notes on track 0 (done by the pattern deserializer), the synth strip on
//! [`Channel::Track`]`(0)` (the mixer deserializer pads the rest) and the preset name looked
//! up from the file's `preset_index`. A version-5 file has no `eq` / `reverb` (the reverb is
//! built from its `reverb_*` params). A version-4 file has a `synth` lane per pattern instead
//! of `notes`; each lane step with a note becomes a length-1 [`crate::pattern::Note`]. A
//! version-3 file has no `mixer`; it loads with [`MixerState::default`]. A version-2 file has
//! a single `pattern`; it becomes pattern A with the chain `[A x1]`, pattern mode, A selected
//! ([`Bank::from_pattern`]). Version 1 (just `version`, `bpm`, `swing`, `master_gain`) still
//! opens with the default bank. Files written by a *newer* format version are refused
//! ([`ProjectError::Version`]).

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::fx::{EqState, ReverbState};
use crate::mixer::MixerState;
use crate::params::ParamId;
use crate::pattern::{Bank, Pattern};
use crate::track::{MAX_TRACKS, TrackId, TrackState, track_index};
use crate::transport::DEFAULT_BPM;

/// File extension for saved projects (`song.beatslopper.json`).
pub const FILE_EXTENSION: &str = "beatslopper.json";

/// Current on-disk format version. Bump when the layout changes incompatibly.
///
/// - 1: envelope only (`version`, `bpm`, `swing`, `master_gain`).
/// - 2: adds `pattern`, `params` (keyed by [`ParamId::key`]), `preset_index`, `octave`,
///   `velocity`; `master_gain` moved into `params`.
/// - 3: `pattern` becomes `bank` (eight patterns with lengths, chain, play mode, current).
/// - 4: adds `mixer` (six strips: level, pan, mute, solo, delay/reverb sends).
/// - 5: each pattern's `synth` lane becomes a polyphonic `notes` list (start, length, pitch,
///   velocity); the lane is derived on load.
/// - 6: adds `eq` (the master parametric EQ, [`EqState`]) and `reverb` (every reverb value,
///   [`ReverbState`]); both `#[serde(default)]`, so a version-5 file loads flat with the
///   reverb built from its `reverb_size` / `reverb_damping` / `reverb_mix` params.
/// - 7: instrument tracks (M10). Adds `tracks` (a [`TrackState`] per active slot holding the
///   per-track params), each pattern's `notes` becomes `tracks` (one note list per track),
///   the mixer grows to 13 strips, `params` keeps only the global ids and `preset_index` is
///   gone (the preset name lives in each track).
/// - 8: FL-style mixer (M11). The mixer gains `inserts` (sixteen
///   [`crate::inserts::InsertState`]s: fader, pan, mute, solo, sends, EQ3, three effect
///   slots, route) and `routes` (each source's insert number). A version-7 (and older) file
///   is migrated with [`MixerState::migrate_pre_m11`]: strip `i` moves onto insert `i + 1`
///   (level, pan, mute, solo and sends), the strip is reset to neutral and the source routed
///   to that insert, so it sounds exactly as before.
pub const FORMAT_VERSION: u32 = 8;

/// Default keyboard octave of a fresh project.
pub const DEFAULT_OCTAVE: i32 = 4;
/// Default keyboard velocity of a fresh project.
pub const DEFAULT_VELOCITY: f32 = 0.8;

/// Everything needed to restore a session.
///
/// `params` holds one entry per global [`ParamId`] (see [`Project::param`] for the
/// default-filling accessor); the per-track parameters live in `tracks`. A `BTreeMap` keeps
/// the JSON sorted, so saving the same project twice yields the same text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    /// On-disk format version ([`FORMAT_VERSION`] when written by this build).
    pub version: u32,
    /// Tempo in beats per minute.
    pub bpm: f32,
    /// Swing amount `0.0..=1.0`.
    pub swing: f32,
    /// The eight patterns, the song chain, the play mode and the selected pattern.
    pub bank: Bank,
    /// Global FX and master parameters keyed by [`ParamId::key`] (delay, reverb, master gain,
    /// limiter). Per-track ids are not stored here since version 7.
    pub params: BTreeMap<String, f32>,
    /// Mixer strips (M6; 13 since M10). Absent in version-3 files, which load with the
    /// defaults; shorter in version 4..6 files, which are padded.
    #[serde(default)]
    pub mixer: MixerState,
    /// Master parametric EQ (plugins milestone, version 6). Absent in older files: flat.
    #[serde(default)]
    pub eq: EqState,
    /// Every reverb value (plugins milestone, version 6). Absent in older files: built from
    /// the `reverb_*` params on load. Size / damping / mix here are authoritative and are
    /// copied into `params` on load so both views agree.
    #[serde(default)]
    pub reverb: ReverbState,
    /// The instrument tracks (M10): one entry per written slot, each naming its `slot`.
    /// Sorted by slot, at most one entry per slot after a load. A file without the field
    /// gets one default `"Synth"` track in slot 0.
    #[serde(default = "default_tracks")]
    pub tracks: Vec<TrackState>,
    /// Keyboard octave (the `Z`/`X` shift).
    pub octave: i32,
    /// Keyboard velocity `0.0..=1.0`.
    pub velocity: f32,
}

impl Default for Project {
    /// The demo beat (pattern A, bassline on track 0) at 120 BPM with every parameter at its
    /// default, one active track `"Synth"`, octave 4, velocity 0.8.
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            bpm: DEFAULT_BPM,
            swing: 0.0,
            bank: Bank::default(),
            params: default_params(),
            mixer: MixerState::default(),
            eq: EqState::default(),
            reverb: ReverbState::default(),
            tracks: default_tracks(),
            octave: DEFAULT_OCTAVE,
            velocity: DEFAULT_VELOCITY,
        }
    }
}

/// Every global parameter at its default value, keyed by [`ParamId::key`].
fn default_params() -> BTreeMap<String, f32> {
    ParamId::ALL
        .iter()
        .filter(|id| id.is_global())
        .map(|id| (id.key().to_owned(), id.default_value()))
        .collect()
}

/// The tracks of a fresh project: one active `"Synth"` in slot 0.
fn default_tracks() -> Vec<TrackState> {
    vec![TrackState::new(0)]
}

impl Project {
    /// Snapshot of the engine's tempo, swing, bank, global parameters, mixer, EQ, reverb and
    /// active tracks plus the UI-side keyboard settings. Track names, colours and preset
    /// names are UI data the engine does not hold: every track gets the defaults of
    /// [`TrackState::new`] (`"Synth"` / `"Track N"`, colour = slot, no preset); decorate them
    /// through [`track_mut`](Self::track_mut) afterwards.
    #[must_use]
    pub fn from_engine(engine: &Engine, octave: i32, velocity: f32) -> Self {
        let params = ParamId::ALL
            .iter()
            .filter(|id| id.is_global())
            .map(|&id| (id.key().to_owned(), engine.param(id)))
            .collect();
        let tracks = engine
            .tracks()
            .iter()
            .enumerate()
            .filter(|(_, t)| t.active)
            .map(|(slot, t)| TrackState::from_track(slot as TrackId, t))
            .collect();
        Self {
            version: FORMAT_VERSION,
            bpm: engine.bpm(),
            swing: engine.transport().swing(),
            bank: engine.bank().clone(),
            params,
            mixer: *engine.mixer(),
            eq: *engine.eq(),
            reverb: *engine.reverb(),
            tracks,
            octave,
            velocity,
        }
    }

    /// A project holding a single pattern (A) with the default chain `[A x1]`, everything
    /// else as in [`Project::default`].
    #[must_use]
    pub fn from_pattern(pattern: Pattern) -> Self {
        Self {
            bank: Bank::from_pattern(pattern),
            ..Self::default()
        }
    }

    /// Pushes tempo, swing, every global parameter, the mixer, the reverb, the EQ, every
    /// track slot and the bank into `engine`.
    ///
    /// Unknown parameter keys are ignored; parameters missing from the file are set to their
    /// defaults, so a loaded project never inherits state from the previous one. Track slots
    /// the project does not list are switched off (voices released, parameters reset). If the
    /// engine's edit track ends up inactive it moves to the first active slot (slot 0 when
    /// there is none). Call before the audio stream starts (or accept a one-off bank copy on
    /// the audio thread).
    ///
    /// The order is load-bearing: [`Engine::set_track_active`]`(slot, false)` resets the
    /// slot's parameters and clears its notes in every pattern, so the listed slots' params go
    /// in first, then the bank, and only then are the unlisted slots switched off (never the
    /// other way round, or a listed slot would lose its params / notes the file holds on an
    /// inactive slot would survive).
    pub fn apply_to(&self, engine: &mut Engine) {
        engine.set_bpm(self.bpm);
        engine.set_swing(self.swing);
        for &id in ParamId::ALL {
            if id.is_global() {
                engine.set_param(id, self.param(id));
            }
        }
        engine.set_mixer_state(&self.mixer);
        // After the params: the reverb state's size / damping / mix are authoritative.
        engine.set_reverb_state(&self.reverb);
        engine.set_eq_state(&self.eq);
        for slot in 0..MAX_TRACKS as TrackId {
            if let Some(state) = self.track(slot).filter(|s| s.active) {
                engine.set_track_active(slot, true);
                for &id in ParamId::ALL {
                    if id.is_track() {
                        engine.set_track_param(slot, id, state.param(id));
                    }
                }
            }
        }
        engine.set_bank(&self.bank);
        // After the bank, so notes the file holds on an inactive slot are dropped too.
        for slot in 0..MAX_TRACKS as TrackId {
            if !self.track(slot).is_some_and(|s| s.active) {
                engine.set_track_active(slot, false);
            }
        }
        if !engine.track_active(engine.edit_track()) {
            let first = (0..MAX_TRACKS as TrackId)
                .find(|&t| engine.track_active(t))
                .unwrap_or(0);
            engine.set_edit_track(first);
        }
    }

    /// Copies the reverb state's size / damping / mix into the matching `params` entries (the
    /// state is the source of truth; the `ParamId` view must agree).
    pub fn sync_reverb_params(&mut self) {
        self.set_param(ParamId::ReverbSize, self.reverb.size);
        self.set_param(ParamId::ReverbDamping, self.reverb.damping);
        self.set_param(ParamId::ReverbMix, self.reverb.mix);
    }

    /// The selected (`current`) pattern of the bank: what the grid edits and pattern mode
    /// plays. Compatibility accessor for code written against the single-pattern (M3)
    /// `Project.pattern` field.
    #[must_use]
    pub fn pattern(&self) -> &Pattern {
        self.bank.current_pattern()
    }

    /// The selected pattern, mutably (see [`pattern`](Self::pattern)).
    pub fn pattern_mut(&mut self) -> &mut Pattern {
        self.bank.current_pattern_mut()
    }

    // --- tracks (M10) ---------------------------------------------------------------------

    /// The state written for track `slot`, if any.
    #[must_use]
    pub fn track(&self, slot: TrackId) -> Option<&TrackState> {
        let slot = track_index(slot) as u8;
        self.tracks.iter().find(|t| t.slot == slot)
    }

    /// The state written for track `slot`, mutably, if any.
    pub fn track_mut(&mut self, slot: TrackId) -> Option<&mut TrackState> {
        let slot = track_index(slot) as u8;
        self.tracks.iter_mut().find(|t| t.slot == slot)
    }

    /// The state of track `slot`, creating a default active entry when none is written.
    pub fn track_or_insert(&mut self, slot: TrackId) -> &mut TrackState {
        let slot = track_index(slot) as u8;
        if let Some(i) = self.tracks.iter().position(|t| t.slot == slot) {
            return &mut self.tracks[i];
        }
        self.tracks.push(TrackState::new(slot));
        self.tracks.sort_by_key(|t| t.slot);
        let i = self.tracks.iter().position(|t| t.slot == slot).unwrap_or(0);
        &mut self.tracks[i]
    }

    /// Per-track parameter `id` of track `slot`, or its default when the slot or the key is
    /// not written.
    #[must_use]
    pub fn track_param(&self, slot: TrackId, id: ParamId) -> f32 {
        self.track(slot)
            .map_or_else(|| id.default_value(), |t| t.param(id))
    }

    /// Sets per-track parameter `id` of track `slot` (creating the entry if needed; global
    /// ids are ignored).
    pub fn set_track_param(&mut self, slot: TrackId, id: ParamId, value: f32) {
        if id.is_track() {
            self.track_or_insert(slot).set_param(id, value);
        }
    }

    /// Value of parameter `id`, or its default when the project does not list it. A
    /// per-track id reads slot 0 (the pre-M10 synth); see
    /// [`track_param`](Self::track_param) for other slots.
    #[must_use]
    pub fn param(&self, id: ParamId) -> f32 {
        if id.is_track() {
            return self.track_param(0, id);
        }
        self.params
            .get(id.key())
            .copied()
            .unwrap_or_else(|| id.default_value())
    }

    /// Sets parameter `id`. A per-track id writes slot 0 (the pre-M10 synth); see
    /// [`set_track_param`](Self::set_track_param) for other slots.
    pub fn set_param(&mut self, id: ParamId, value: f32) {
        if id.is_track() {
            self.set_track_param(0, id, value);
        } else {
            self.params.insert(id.key().to_owned(), value);
        }
    }

    /// Pretty-printed JSON (two-space indent, trailing newline).
    ///
    /// Serializing a `Project` cannot fail (no maps with non-string keys, and the custom
    /// pattern/bank serializers only emit plain arrays), so this is infallible.
    #[must_use]
    pub fn to_json_pretty(&self) -> String {
        let mut text = serde_json::to_string_pretty(self)
            .expect("Project serialization is infallible: plain structs, string-keyed map");
        text.push('\n');
        text
    }

    /// Parses a project. Older format versions are migrated (see the module docs; a
    /// version-6 file becomes one `"Synth"` track in slot 0); a newer version than
    /// [`FORMAT_VERSION`] is an error. The bank, mixer, EQ, reverb and tracks are normalised
    /// (indices, repeats, lengths and values clamped; duplicate track slots dropped).
    pub fn from_json(text: &str) -> Result<Project, ProjectError> {
        let mut value: serde_json::Value = serde_json::from_str(text)?;
        let version = value
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .map_or(FORMAT_VERSION, |v| u32::try_from(v).unwrap_or(u32::MAX));
        if version > FORMAT_VERSION {
            return Err(ProjectError::Version {
                found: version,
                supported: FORMAT_VERSION,
            });
        }
        // Version 1 kept the master gain as a top-level field.
        let legacy_master_gain = (version < 2)
            .then(|| value.get("master_gain").and_then(serde_json::Value::as_f64))
            .flatten();
        // Versions 1 and 2 had a single `pattern` instead of the `bank`.
        let legacy_pattern = match value.as_object_mut() {
            Some(obj) if version < 3 && !obj.contains_key("bank") => obj
                .remove("pattern")
                .map(serde_json::from_value::<Pattern>)
                .transpose()?,
            _ => None,
        };
        // Versions before 6 had no `reverb`: build it from the reverb params.
        let has_reverb = value.get("reverb").is_some();
        // Versions before 7 had one synth: its track params sit in `params` and its preset
        // is an index into the built-in list.
        let legacy_preset = (version < 7)
            .then(|| {
                value
                    .get("preset_index")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|i| usize::try_from(i).ok())
                    .and_then(|i| crate::presets::ALL.get(i))
                    .map(|p| p.name.to_owned())
            })
            .flatten();
        let legacy_track_params: Option<BTreeMap<String, f32>> = (version < 7)
            .then(|| {
                value
                    .get("params")
                    .and_then(serde_json::Value::as_object)
                    .map(|map| {
                        map.iter()
                            .filter_map(|(k, v)| {
                                let id = ParamId::from_key(k).filter(|id| id.is_track())?;
                                v.as_f64().map(|v| (id.key().to_owned(), v as f32))
                            })
                            .collect()
                    })
            })
            .flatten();
        // Versions before 8 kept the mix on the source strips (M11 moves it onto the
        // inserts); remember how many strips the file actually wrote.
        let legacy_strips = (version < 8).then(|| {
            value
                .get("mixer")
                .and_then(|m| m.get("strips"))
                .and_then(serde_json::Value::as_array)
                .map_or(0, Vec::len)
        });

        let mut project: Project = serde_json::from_value(value)?;
        project.version = FORMAT_VERSION;
        if version < 7 {
            let mut synth = TrackState::new(0);
            if let Some(params) = legacy_track_params {
                for (key, v) in params {
                    synth.params.insert(key, v);
                }
            }
            if let Some(name) = legacy_preset {
                synth.preset = name;
            }
            project.tracks = vec![synth];
        }
        project.normalize_params();
        project.normalize_tracks();
        if let Some(gain) = legacy_master_gain {
            project.set_param(ParamId::MasterGain, gain as f32);
        }
        if let Some(pattern) = legacy_pattern {
            project.bank = Bank::from_pattern(pattern);
        }
        project.bank.normalise();
        if let Some(listed) = legacy_strips {
            project.mixer.migrate_pre_m11(listed);
        }
        project.mixer.normalise();
        project.eq.normalise();
        project.reverb.normalise();
        if !has_reverb {
            project.reverb.size = project.param(ParamId::ReverbSize);
            project.reverb.damping = project.param(ParamId::ReverbDamping);
            project.reverb.mix = project.param(ParamId::ReverbMix);
            project.reverb.normalise();
        }
        project.sync_reverb_params();
        Ok(project)
    }

    /// Drops unknown and per-track parameter keys and fills missing global ones with their
    /// defaults, so `params` holds exactly one entry per global [`ParamId`].
    fn normalize_params(&mut self) {
        self.params
            .retain(|key, _| ParamId::from_key(key).is_some_and(ParamId::is_global));
        for &id in ParamId::ALL {
            if id.is_global() {
                self.params
                    .entry(id.key().to_owned())
                    .or_insert_with(|| id.default_value());
            }
        }
    }

    /// Normalises every track state, drops later duplicates of a slot and sorts by slot.
    fn normalize_tracks(&mut self) {
        for t in &mut self.tracks {
            t.normalise();
        }
        let mut seen = [false; MAX_TRACKS];
        self.tracks.retain(|t| {
            let slot = track_index(t.slot);
            !std::mem::replace(&mut seen[slot], true)
        });
        self.tracks.sort_by_key(|t| t.slot);
    }

    /// Writes the project as pretty JSON to `path` (overwriting).
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ProjectError> {
        std::fs::write(path, self.to_json_pretty())?;
        Ok(())
    }

    /// Reads and parses a project file.
    pub fn load(path: impl AsRef<Path>) -> Result<Project, ProjectError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_json(&text)
    }
}

/// `true` when `path` ends in `.beatslopper.json` (case-insensitive).
#[must_use]
pub fn is_project_path(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            lower.len() > FILE_EXTENSION.len() + 1
                && lower.ends_with(FILE_EXTENSION)
                && lower.as_bytes()[lower.len() - FILE_EXTENSION.len() - 1] == b'.'
        })
}

/// Why a project could not be saved or loaded.
#[derive(Debug)]
pub enum ProjectError {
    /// Reading or writing the file failed.
    Io(std::io::Error),
    /// The text is not valid project JSON.
    Json(serde_json::Error),
    /// The file was written by a newer BeatSlopper.
    Version {
        /// Version found in the file.
        found: u32,
        /// Newest version this build reads ([`FORMAT_VERSION`]).
        supported: u32,
    },
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectError::Io(e) => write!(f, "project file I/O error: {e}"),
            ProjectError::Json(e) => write!(f, "invalid project JSON: {e}"),
            ProjectError::Version { found, supported } => write!(
                f,
                "project format version {found} is newer than the supported version {supported}"
            ),
        }
    }
}

impl std::error::Error for ProjectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProjectError::Io(e) => Some(e),
            ProjectError::Json(e) => Some(e),
            ProjectError::Version { .. } => None,
        }
    }
}

impl From<std::io::Error> for ProjectError {
    fn from(e: std::io::Error) -> Self {
        ProjectError::Io(e)
    }
}

impl From<serde_json::Error> for ProjectError {
    fn from(e: serde_json::Error) -> Self {
        ProjectError::Json(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{ChainEntry, PlayMode};

    /// Number of global parameter ids.
    fn global_count() -> usize {
        ParamId::ALL.iter().filter(|id| id.is_global()).count()
    }

    #[test]
    fn default_project_lists_every_global_param_once_and_one_track() {
        let p = Project::default();
        assert_eq!(p.version, FORMAT_VERSION);
        assert_eq!(p.params.len(), global_count());
        for &id in ParamId::ALL {
            assert_eq!(p.param(id), id.default_value(), "{id:?}");
            if id.is_track() {
                assert!(!p.params.contains_key(id.key()), "{id:?} is per track");
            }
        }
        assert_eq!(p.tracks, vec![TrackState::new(0)]);
        assert_eq!(p.track(0).unwrap().name, "Synth");
        assert!(p.track(1).is_none());
        assert_eq!(p.bank, Bank::default());
        assert_eq!(p.pattern(), &Pattern::demo());
        assert_eq!(p.pattern().notes(0).len(), 9);
    }

    #[test]
    fn json_is_pretty_and_sorted() {
        let text = Project::default().to_json_pretty();
        let head = format!("{{\n  \"version\": {FORMAT_VERSION},");
        assert!(text.starts_with(&head));
        assert!(text.ends_with("}\n"));
        let delay = text.find("\"delay_feedback\"").unwrap();
        let master = text.find("\"master_gain\"").unwrap();
        assert!(delay < master, "params must be alphabetically sorted");
        assert!(text.contains("\"mode\": \"pattern\""));
        assert!(text.contains("\"mixer\": {\n    \"strips\": ["));
        assert!(text.contains("\"reverb_send\": 0.3"));
        assert!(text.contains("\"tracks\": [\n    {\n      \"slot\": 0,"));
        assert!(!text.contains("preset_index"));
    }

    #[test]
    fn version_3_file_loads_with_the_default_mixer() {
        let mut v3 = serde_json::to_value(Project::default()).unwrap();
        v3["version"] = serde_json::json!(3);
        v3.as_object_mut().unwrap().remove("mixer");
        let p = Project::from_json(&v3.to_string()).unwrap();
        assert_eq!(p.version, FORMAT_VERSION);
        assert_eq!(p.mixer, MixerState::default());
        // Out-of-range mixer values are normalised on load.
        let mut v = serde_json::to_value(Project::default()).unwrap();
        v["mixer"]["strips"][0]["level_db"] = serde_json::json!(99.0);
        v["mixer"]["strips"][5]["pan"] = serde_json::json!(-4.0);
        let p = Project::from_json(&v.to_string()).unwrap();
        assert_eq!(p.mixer.strips[0].level_db, crate::mixer::MAX_LEVEL_DB);
        assert_eq!(p.mixer.strips[5].pan, -1.0);
    }

    #[test]
    fn version_1_envelope_loads_with_defaults() {
        let v1 = r#"{ "version": 1, "bpm": 100.0, "swing": 0.25, "master_gain": 0.5 }"#;
        let p = Project::from_json(v1).unwrap();
        assert_eq!(p.version, FORMAT_VERSION);
        assert_eq!(p.bpm, 100.0);
        assert_eq!(p.swing, 0.25);
        assert_eq!(p.param(ParamId::MasterGain), 0.5);
        assert_eq!(p.bank, Bank::default());
        assert_eq!(p.octave, DEFAULT_OCTAVE);
        assert_eq!(p.tracks, vec![TrackState::new(0)]);
    }

    #[test]
    fn version_2_pattern_becomes_pattern_a() {
        let mut pattern = Pattern::empty();
        pattern.set_drum_step(crate::drums::DrumRow::Kick, 3, true, true);
        let v2 = serde_json::json!({
            "version": 2,
            "bpm": 95.0,
            "pattern": pattern,
        });
        let p = Project::from_json(&v2.to_string()).unwrap();
        assert_eq!(p.version, FORMAT_VERSION);
        assert_eq!(p.bpm, 95.0);
        assert_eq!(p.bank.patterns[0], pattern);
        assert!(p.bank.patterns[1..].iter().all(Pattern::is_empty));
        assert_eq!(p.bank.chain_entries(), &[ChainEntry::new(0, 1)]);
        assert_eq!(p.bank.mode, PlayMode::Pattern);
        assert_eq!(p.bank.current, 0);
        // A version-2 file whose pattern is missing gets the default bank.
        let p = Project::from_json(r#"{ "version": 2 }"#).unwrap();
        assert_eq!(p.bank, Bank::default());
        // A version-3 file ignores a stray legacy `pattern`.
        let v3 = serde_json::json!({ "version": 3, "pattern": pattern, "bank": Bank::empty() });
        let p = Project::from_json(&v3.to_string()).unwrap();
        assert_eq!(p.bank, Bank::empty());
    }

    #[test]
    fn version_6_synth_becomes_track_zero() {
        // A v6 file: every param (track ids included) in `params`, `preset_index`, six strips.
        let mut v6 = serde_json::to_value(Project::default()).unwrap();
        v6["version"] = serde_json::json!(6);
        v6.as_object_mut().unwrap().remove("tracks");
        v6["preset_index"] = serde_json::json!(4);
        v6["params"][ParamId::FilterCutoff.key()] = serde_json::json!(812.5);
        v6["params"][ParamId::LofiMix.key()] = serde_json::json!(0.7);
        v6["params"][ParamId::DelayFeedback.key()] = serde_json::json!(0.6);
        let strips: Vec<serde_json::Value> = v6["mixer"]["strips"]
            .as_array()
            .unwrap()
            .iter()
            .take(6)
            .cloned()
            .collect();
        v6["mixer"]["strips"] = serde_json::Value::Array(strips);
        v6["mixer"]["strips"][5]["level_db"] = serde_json::json!(-4.5);
        let p = Project::from_json(&v6.to_string()).unwrap();
        assert_eq!(p.version, FORMAT_VERSION);
        assert_eq!(p.tracks.len(), 1);
        let synth = p.track(0).unwrap();
        assert_eq!(synth.name, "Synth");
        assert_eq!(synth.colour, 0);
        assert!(synth.active);
        assert_eq!(synth.preset, crate::presets::ALL[4].name);
        assert_eq!(synth.param(ParamId::FilterCutoff), 812.5);
        assert_eq!(synth.param(ParamId::LofiMix), 0.7);
        assert_eq!(p.param(ParamId::DelayFeedback), 0.6);
        assert_eq!(p.params.len(), global_count());
        assert!(!p.params.contains_key(ParamId::FilterCutoff.key()));
        // The synth strip is track 0's; M11 moves the mix onto insert 6 and leaves the
        // strip neutral; the unlisted tracks get the insert defaults.
        assert_eq!(
            p.mixer.insert(6).level_db,
            -4.5,
            "the synth strip is track 0's"
        );
        assert_eq!(p.mixer.strips[5].level_db, 0.0);
        assert_eq!(p.mixer.route(crate::mixer::Channel::Track(0)), 6);
        assert_eq!(
            p.mixer.strips[6],
            crate::mixer::Strip::for_channel(crate::mixer::Channel::Track(1))
        );
        assert_eq!(
            p.mixer.insert(7),
            &crate::inserts::InsertState::default_for(7)
        );
        assert_eq!(p.pattern().notes(0).len(), 9);
        // An unknown preset index gives no preset name.
        v6["preset_index"] = serde_json::json!(99);
        let p = Project::from_json(&v6.to_string()).unwrap();
        assert_eq!(p.track(0).unwrap().preset, "");
    }

    #[test]
    fn version_7_tracks_normalise_and_round_trip() {
        let mut p = Project::default();
        p.set_track_param(3, ParamId::Osc1Mix, 0.25);
        p.track_mut(3).unwrap().name = "Keys".to_owned();
        p.track_or_insert(1).active = false;
        assert_eq!(
            p.tracks.iter().map(|t| t.slot).collect::<Vec<_>>(),
            [0, 1, 3]
        );
        assert_eq!(p.track_param(3, ParamId::Osc1Mix), 0.25);
        assert_eq!(
            p.track_param(5, ParamId::Osc1Mix),
            ParamId::Osc1Mix.default_value()
        );
        let json = p.to_json_pretty();
        let back = Project::from_json(&json).unwrap();
        assert_eq!(back, p);
        // Duplicate slots: the first wins; a missing `tracks` field means one Synth; an
        // explicit empty list means none.
        let mut v = serde_json::to_value(&p).unwrap();
        v["tracks"] = serde_json::json!([
            { "slot": 2, "name": "A" },
            { "slot": 2, "name": "B" },
            { "slot": 9, "name": "wraps to 1" }
        ]);
        let dup = Project::from_json(&v.to_string()).unwrap();
        assert_eq!(dup.tracks.len(), 2);
        assert_eq!(dup.tracks[0].slot, 1);
        assert_eq!(dup.tracks[1].name, "A");
        v.as_object_mut().unwrap().remove("tracks");
        let none = Project::from_json(&v.to_string()).unwrap();
        assert_eq!(none.tracks, vec![TrackState::new(0)]);
        v["tracks"] = serde_json::json!([]);
        let empty = Project::from_json(&v.to_string()).unwrap();
        assert!(empty.tracks.is_empty());
        // Track ids in a v7 `params` map are dropped.
        v["params"][ParamId::FilterCutoff.key()] = serde_json::json!(1.0);
        let p = Project::from_json(&v.to_string()).unwrap();
        assert!(!p.params.contains_key(ParamId::FilterCutoff.key()));
    }

    #[test]
    fn newer_version_is_refused_and_garbage_is_a_json_error() {
        let err =
            Project::from_json(&format!(r#"{{ "version": {} }}"#, FORMAT_VERSION + 1)).unwrap_err();
        assert!(matches!(
            err,
            ProjectError::Version {
                found: 9,
                supported: FORMAT_VERSION
            }
        ));
        assert!(err.to_string().contains("newer"));
        let err = Project::from_json("not json").unwrap_err();
        assert!(matches!(err, ProjectError::Json(_)));
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn pattern_accessors_follow_current() {
        let mut p = Project::default();
        p.bank.current = 2;
        p.pattern_mut().set_synth_step(0, 0, Some(50), false);
        assert_eq!(p.bank.patterns[2].synth[0].note, Some(50));
        assert_eq!(p.pattern(), &p.bank.patterns[2]);
        let single = Project::from_pattern(Pattern::empty());
        assert_eq!(single.bank, Bank::empty());
    }

    #[test]
    fn project_paths() {
        assert!(is_project_path("song.beatslopper.json"));
        assert!(is_project_path("C:/beats/My Song.BeatSlopper.JSON"));
        assert!(!is_project_path("song.json"));
        assert!(!is_project_path(".beatslopper.json"));
        assert!(!is_project_path("beatslopper.json"));
        assert!(!is_project_path("song.wav"));
    }

    #[test]
    fn load_missing_file_is_an_io_error() {
        let err = Project::load("Z:/definitely/not/here.beatslopper.json").unwrap_err();
        assert!(matches!(err, ProjectError::Io(_)));
    }
}
