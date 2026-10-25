//! Project file (M3/M5/M6/M7): `*.beatslopper.json` holding BPM, swing, the pattern bank
//! (eight patterns with their notes, chain, play mode), synth/FX params, the mixer, preset and
//! keyboard settings.
//!
//! Serialization happens on the UI thread only; the audio thread receives the loaded state
//! through [`Project::apply_to`] (before the stream starts) or as small `Copy` messages.
//!
//! Layout ([`FORMAT_VERSION`] 5), human-readable and stable:
//!
//! ```json
//! {
//!   "version": 6,
//!   "bpm": 120.0,
//!   "swing": 0.0,
//!   "bank": {
//!     "patterns": [
//!       { "length": 16, "drums": [[...], [...], [...], [...], [...]],
//!         "notes": [ { "start": 0, "length": 1, "pitch": 36, "velocity": 0.8 } ],
//!         "drum_volume": [...], "drum_tune": [...] },
//!       "... 8 patterns in all (A..H)"
//!     ],
//!     "chain": [ { "pattern": 0, "repeats": 1 } ],
//!     "mode": "pattern",
//!     "current": 0
//!   },
//!   "params": { "amp_attack": 0.01, "osc1_wave": 1.0, "...": 0.0 },
//!   "mixer": {
//!     "strips": [
//!       { "level_db": 0.0, "pan": 0.0, "mute": false, "solo": false,
//!         "delay_send": 0.0, "reverb_send": 0.0 },
//!       "... 6 strips in all: kick, snare, clap, closed hat, open hat, synth"
//!     ]
//!   },
//!   "preset_index": 0,
//!   "octave": 4,
//!   "velocity": 0.8
//! }
//! ```
//!
//! Each drum row is a plain array of 16, 32 or 64 step objects and `notes` lists the
//! pattern's synth notes (see [`crate::pattern`] for the exact rules; the derived `synth`
//! lane is not written). Parameters are keyed by [`ParamId::key`] so renaming a UI label
//! never breaks a file. On load, unknown parameter keys are ignored and missing ones take
//! [`ParamId::default_value`]; missing top-level fields take their [`Project::default`]
//! value.
//!
//! Migration: a version-4 file has a `synth` lane per pattern instead of `notes`; each lane
//! step with a note becomes a length-1 [`crate::pattern::Note`] (velocity 1.0 when accented,
//! 0.8 otherwise), handled by the pattern deserializer. A version-3 file has no `mixer`; it
//! loads with [`MixerState::default`] (unity, centre, synth sent to delay and reverb,
//! snare/clap lightly to the reverb), which plays the way M5 did. A version-2 file has a
//! single `pattern`; it becomes pattern A with the chain `[A x1]`, pattern mode, A selected
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
pub const FORMAT_VERSION: u32 = 6;

/// Default keyboard octave of a fresh project.
pub const DEFAULT_OCTAVE: i32 = 4;
/// Default keyboard velocity of a fresh project.
pub const DEFAULT_VELOCITY: f32 = 0.8;

/// Everything needed to restore a session.
///
/// `params` holds one entry per [`ParamId`] (see [`Project::param`] for the default-filling
/// accessor). A `BTreeMap` keeps the JSON sorted, so saving the same project twice yields the
/// same text.
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
    /// Synth, FX and master parameters keyed by [`ParamId::key`].
    pub params: BTreeMap<String, f32>,
    /// Mixer strips (M6). Absent in version-3 files, which load with the defaults.
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
    /// Index of the selected built-in preset.
    pub preset_index: usize,
    /// Keyboard octave (the `Z`/`X` shift).
    pub octave: i32,
    /// Keyboard velocity `0.0..=1.0`.
    pub velocity: f32,
}

impl Default for Project {
    /// The demo beat (pattern A) at 120 BPM with every parameter at its default, preset 0,
    /// octave 4, velocity 0.8.
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
            preset_index: 0,
            octave: DEFAULT_OCTAVE,
            velocity: DEFAULT_VELOCITY,
        }
    }
}

/// Every parameter at its default value, keyed by [`ParamId::key`].
fn default_params() -> BTreeMap<String, f32> {
    ParamId::ALL
        .iter()
        .map(|id| (id.key().to_owned(), id.default_value()))
        .collect()
}

impl Project {
    /// Snapshot of the engine's tempo, swing, bank, parameters and mixer plus the UI-side
    /// preset and keyboard settings.
    #[must_use]
    pub fn from_engine(engine: &Engine, preset_index: usize, octave: i32, velocity: f32) -> Self {
        let params = ParamId::ALL
            .iter()
            .map(|&id| (id.key().to_owned(), engine.param(id)))
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
            preset_index,
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

    /// Pushes tempo, swing, every known parameter, the mixer and the bank into `engine`.
    ///
    /// Unknown parameter keys are ignored; parameters missing from the file are set to their
    /// defaults, so a loaded project never inherits state from the previous one. Call before
    /// the audio stream starts (or accept a one-off bank copy on the audio thread).
    pub fn apply_to(&self, engine: &mut Engine) {
        engine.set_bpm(self.bpm);
        engine.set_swing(self.swing);
        for &id in ParamId::ALL {
            engine.set_param(id, self.param(id));
        }
        engine.set_mixer_state(&self.mixer);
        // After the params: the reverb state's size / damping / mix are authoritative.
        engine.set_reverb_state(&self.reverb);
        engine.set_eq_state(&self.eq);
        engine.set_bank(&self.bank);
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

    /// Value of parameter `id`, or its default when the project does not list it.
    #[must_use]
    pub fn param(&self, id: ParamId) -> f32 {
        self.params
            .get(id.key())
            .copied()
            .unwrap_or_else(|| id.default_value())
    }

    /// Sets parameter `id`.
    pub fn set_param(&mut self, id: ParamId, value: f32) {
        self.params.insert(id.key().to_owned(), value);
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

    /// Parses a project. Older format versions are migrated (version 2's single `pattern`
    /// becomes pattern A; missing fields default); a newer version than [`FORMAT_VERSION`] is
    /// an error. The bank is normalised (indices, repeats and lengths clamped).
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

        let mut project: Project = serde_json::from_value(value)?;
        project.version = FORMAT_VERSION;
        project.normalize_params();
        if let Some(gain) = legacy_master_gain {
            project.set_param(ParamId::MasterGain, gain as f32);
        }
        if let Some(pattern) = legacy_pattern {
            project.bank = Bank::from_pattern(pattern);
        }
        project.bank.normalise();
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

    /// Drops unknown parameter keys and fills missing ones with their defaults, so `params`
    /// holds exactly one entry per [`ParamId`].
    fn normalize_params(&mut self) {
        self.params
            .retain(|key, _| ParamId::from_key(key).is_some());
        for &id in ParamId::ALL {
            self.params
                .entry(id.key().to_owned())
                .or_insert_with(|| id.default_value());
        }
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

    #[test]
    fn default_project_lists_every_param_once() {
        let p = Project::default();
        assert_eq!(p.version, FORMAT_VERSION);
        assert_eq!(p.params.len(), ParamId::COUNT);
        for &id in ParamId::ALL {
            assert_eq!(p.param(id), id.default_value(), "{id:?}");
        }
        assert_eq!(p.bank, Bank::default());
        assert_eq!(p.pattern(), &Pattern::demo());
    }

    #[test]
    fn json_is_pretty_and_sorted() {
        let text = Project::default().to_json_pretty();
        let head = format!("{{\n  \"version\": {FORMAT_VERSION},");
        assert!(text.starts_with(&head));
        assert!(text.ends_with("}\n"));
        let amp = text.find("\"amp_attack\"").unwrap();
        let osc = text.find("\"osc1_wave\"").unwrap();
        assert!(amp < osc, "params must be alphabetically sorted");
        assert!(text.contains("\"mode\": \"pattern\""));
        assert!(text.contains("\"mixer\": {\n    \"strips\": ["));
        assert!(text.contains("\"reverb_send\": 0.3"));
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
        let mut v5 = serde_json::to_value(Project::default()).unwrap();
        v5["mixer"]["strips"][0]["level_db"] = serde_json::json!(99.0);
        v5["mixer"]["strips"][5]["pan"] = serde_json::json!(-4.0);
        let p = Project::from_json(&v5.to_string()).unwrap();
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
    fn newer_version_is_refused_and_garbage_is_a_json_error() {
        let err =
            Project::from_json(&format!(r#"{{ "version": {} }}"#, FORMAT_VERSION + 1)).unwrap_err();
        assert!(matches!(
            err,
            ProjectError::Version {
                found: 7,
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
        p.pattern_mut().set_synth_step(0, Some(50), false);
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
